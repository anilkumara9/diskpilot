//! Drive health I/O: the Windows SMART reads behind
//! `get_drive_health`.
//!
//! The core crate owns the byte-level parsers and the health
//! classification (`diskpilot_core::smart`); this module owns only
//! the device IOCTLs that fetch the raw payloads:
//!
//! - `IOCTL_STORAGE_QUERY_PROPERTY` for the model + bus type,
//! - `IOCTL_DISK_GET_LENGTH_INFO` for capacity,
//! - `IOCTL_STORAGE_PREDICT_FAILURE` for the drive's own verdict,
//! - `IOCTL_STORAGE_PROTOCOL_COMMAND` (NVMe Get Log Page 02h) for
//!   NVMe health, and
//! - `IOCTL_ATA_PASS_THROUGH_DIRECT` (SMART READ DATA, 0xB0/0xD0)
//!   for ATA/SATA health.
//!
//! Everything degrades per drive, never wholesale: a drive that
//! cannot even be described is skipped, a drive whose SMART read
//! fails is reported with `Unknown` vitals, and off Windows the
//! answer is simply an empty list. Nothing here writes to a device.
//!
//! NOTE: these reads are compile-verified only — no Windows machine
//! was available to run them. The parsers they feed are fixture-
//! tested in the core.

use diskpilot_core::smart::DriveHealth;

/// Health of every physical drive the OS exposes
/// (`\\.\PhysicalDrive0` … `15`). Long device probes run on a
/// blocking worker thread: a wedged drive must not freeze the UI.
#[tauri::command]
pub async fn get_drive_health() -> Vec<DriveHealth> {
    tauri::async_runtime::spawn_blocking(collect_drive_health)
        .await
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn collect_drive_health() -> Vec<DriveHealth> {
    Vec::new()
}

#[cfg(windows)]
fn collect_drive_health() -> Vec<DriveHealth> {
    (0..16).filter_map(win::probe_drive).collect()
}

// ---------------------------------------------------------------------------
// NVMe protocol-command request layout
// ---------------------------------------------------------------------------

/// `STORAGE_PROTOCOL_COMMAND_FLAG_ADAPTER_REQUEST` (winioctl.h):
/// the command is addressed to the adapter/controller, not a LUN.
const STORAGE_PROTOCOL_COMMAND_FLAG_ADAPTER_REQUEST: u32 = 0x8000_0000;

/// `STORAGE_PROTOCOL_STATUS_SUCCESS` (winioctl.h): the value the
/// driver writes to the request's ReturnStatus field when the
/// protocol command completes successfully.
const STORAGE_PROTOCOL_STATUS_SUCCESS: u32 = 0x1;

/// Offset of the NVMe admin command inside the request buffer,
/// and of the data region behind it. The command is 64 bytes, so
/// the data starts at 80 + 64 = 144 and the buffer totals
/// 144 + 512 = 656 bytes.
const NVME_COMMAND_OFFSET: usize = 80;
const NVME_DATA_OFFSET: usize = 144;
const NVME_LOG_PAGE_LEN: usize = 512;

/// Build the `IOCTL_STORAGE_PROTOCOL_COMMAND` request for an NVMe
/// Get Log Page of the SMART/Health log (LID 02h, 512 bytes). Pure
/// byte layout with no OS dependency, so the offsets are
/// unit-tested on any host.
///
/// Field map (winioctl.h `STORAGE_PROTOCOL_COMMAND`): Version = 1
/// @0, Length = 84 @4 (the structure size including `Command[1]`),
/// ProtocolType = 3 (Nvme) @8, Flags = ADAPTER_REQUEST @12,
/// CommandLength = 64 @24, DataFromDeviceTransferLength = 512
/// @36, TimeOutValue = 10 @40 (seconds),
/// DataFromDeviceBufferOffset = 144 @52. The driver answers in
/// ReturnStatus @16 and ErrorCode @20. The 64-byte NVMe admin
/// command sits at @80: CDW0 (opcode 02h, Get Log Page) @80,
/// NSID 0xFFFFFFFF @84 (the log is controller-wide),
/// CDW10 = LID 02h with NUMD (127 dwords − 1) in bits 16..=31
/// @120, CDW11 = 0 @124.
fn build_nvme_health_request() -> Vec<u8> {
    let mut buffer = vec![0u8; NVME_DATA_OFFSET + NVME_LOG_PAGE_LEN];
    let put_u32 = |buffer: &mut [u8], off: usize, value: u32| {
        buffer[off..off + 4].copy_from_slice(&value.to_le_bytes());
    };
    put_u32(&mut buffer, 0, 1); // Version
    put_u32(&mut buffer, 4, 84); // Length
    put_u32(&mut buffer, 8, 3); // ProtocolType = ProtocolTypeNvme
    put_u32(&mut buffer, 12, STORAGE_PROTOCOL_COMMAND_FLAG_ADAPTER_REQUEST);
    put_u32(&mut buffer, 24, 64); // CommandLength
    put_u32(&mut buffer, 36, NVME_LOG_PAGE_LEN as u32); // DataFromDeviceTransferLength
    put_u32(&mut buffer, 40, 10); // TimeOutValue (seconds)
    put_u32(&mut buffer, 52, NVME_DATA_OFFSET as u32); // DataFromDeviceBufferOffset
    put_u32(&mut buffer, NVME_COMMAND_OFFSET, 0x02); // CDW0: Get Log Page
    put_u32(&mut buffer, NVME_COMMAND_OFFSET + 4, 0xFFFF_FFFF); // NSID
    // CDW10: LID 02h in the low byte, NUMD (dwords − 1 = 127) in
    // bits 16..=31 — 512 bytes total. CDW11 stays zero.
    put_u32(
        &mut buffer,
        NVME_COMMAND_OFFSET + 40,
        0x02 | (127 << 16),
    );
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvme_health_request_matches_winioctl_layout() {
        let buffer = build_nvme_health_request();
        let get = |off: usize| u32::from_le_bytes(buffer[off..off + 4].try_into().unwrap());
        assert_eq!(buffer.len(), 656);
        assert_eq!(get(0), 1, "Version");
        assert_eq!(get(4), 84, "Length");
        assert_eq!(get(8), 3, "ProtocolType = Nvme");
        assert_eq!(
            get(12),
            STORAGE_PROTOCOL_COMMAND_FLAG_ADAPTER_REQUEST,
            "Flags = ADAPTER_REQUEST"
        );
        assert_eq!(get(24), 64, "CommandLength");
        assert_eq!(get(36), 512, "DataFromDeviceTransferLength");
        assert_eq!(get(40), 10, "TimeOutValue");
        assert_eq!(get(52), 144, "DataFromDeviceBufferOffset");
        assert_eq!(get(80), 0x02, "CDW0 opcode = Get Log Page");
        assert_eq!(get(84), 0xFFFF_FFFF, "NSID");
        assert_eq!(get(120), 0x02 | (127 << 16), "CDW10 = LID | NUMD");
        assert_eq!(get(124), 0, "CDW11");
        // The response fields start zeroed and the log-page region
        // is untouched buffer space.
        assert_eq!(get(16), 0, "ReturnStatus starts zeroed");
        assert_eq!(get(20), 0, "ErrorCode starts zeroed");
        assert!(buffer[NVME_DATA_OFFSET..].iter().all(|b| *b == 0));
    }
}

/// A drive that opened and described itself but would not answer
/// SMART: honest `Unknown` vitals (with the drive's own failure
/// prediction, when even that much could be read).
#[cfg(windows)]
fn unknown_health(
    model: String,
    interface: String,
    capacity_bytes: u64,
    predicted_failure: bool,
) -> DriveHealth {
    DriveHealth {
        model,
        interface,
        capacity_bytes,
        temperature_c: None,
        life_left_percent: None,
        power_on_hours: None,
        power_cycles: None,
        reallocated_sectors: None,
        predicted_failure,
        status: diskpilot_core::smart::classify(None, None, None, None, predicted_failure),
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use diskpilot_core::smart::{
        health_from_ata, health_from_nvme, parse_ata_attributes, parse_nvme_health, DriveHealth,
    };

    use super::{
        build_nvme_health_request, unknown_health, NVME_DATA_OFFSET, NVME_LOG_PAGE_LEN,
        STORAGE_PROTOCOL_STATUS_SUCCESS,
    };

    type Handle = *mut c_void;

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;

    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
    const IOCTL_STORAGE_PREDICT_FAILURE: u32 = 0x002D_1100;
    const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405C;
    const IOCTL_STORAGE_PROTOCOL_COMMAND: u32 = 0x002D_D480;
    const IOCTL_ATA_PASS_THROUGH_DIRECT: u32 = 0x0004_D030;

    // STORAGE_BUS_TYPE values (ntddstor.h).
    const BUS_TYPE_SCSI: i32 = 1;
    const BUS_TYPE_ATA: i32 = 3;
    const BUS_TYPE_USB: i32 = 7;
    const BUS_TYPE_SATA: i32 = 11;
    const BUS_TYPE_NVME: i32 = 17;
    const BUS_TYPE_ISCSI: i32 = 9;
    const BUS_TYPE_SAS: i32 = 10;

    #[link(name = "Kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn DeviceIoControl(
            device: Handle,
            control_code: u32,
            in_buffer: *const c_void,
            in_buffer_size: u32,
            out_buffer: *mut c_void,
            out_buffer_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    /// Owns a device handle so every probe path closes it.
    struct HandleGuard(Handle);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn wide_null(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn open_drive(path: &[u16], access: u32) -> Option<HandleGuard> {
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle.is_null() || handle as isize == -1 {
            None
        } else {
            Some(HandleGuard(handle))
        }
    }

    /// One DeviceIoControl round trip over same-sized in/out
    /// buffers. On success, returns how many bytes the driver
    /// actually wrote — callers that parse the buffer bound their
    /// reads by it, never by the buffer's capacity.
    fn ioctl(handle: Handle, code: u32, buffer: &mut [u8]) -> Option<u32> {
        let mut returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                code,
                buffer.as_ptr() as *const c_void,
                buffer.len() as u32,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            Some(returned)
        } else {
            None
        }
    }

    /// `(model, bus type)` from the storage device descriptor. The
    /// descriptor is a fixed header followed by NUL-terminated
    /// strings at absolute offsets; the product id sits at the
    /// offset stored in bytes 16..20, the bus type at 28..32.
    fn device_descriptor(handle: Handle) -> Option<(String, i32)> {
        // STORAGE_PROPERTY_QUERY: PropertyId 0
        // (StorageDeviceProperty), QueryType 0 (PropertyStandardQuery).
        // The buffer is generous (descriptors with long string sets
        // overrun a 1 KB buffer) and every read is bounded by what
        // the driver actually returned, not the capacity.
        let mut buffer = vec![0u8; 4096];
        let returned = ioctl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &mut buffer)? as usize;
        // The fields read here end at the bus type (bytes 28..32).
        if returned < 32 {
            return None;
        }
        let limit = returned.min(buffer.len());
        let read_u32 = |off: usize| -> u32 {
            u32::from_le_bytes(buffer[off..off + 4].try_into().unwrap_or([0; 4]))
        };
        let product_offset = read_u32(16) as usize;
        let bus_type = i32::from_le_bytes(buffer[28..32].try_into().unwrap_or([0; 4]));
        let model = if product_offset > 0 && product_offset < limit {
            let tail = &buffer[product_offset..limit];
            let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
            String::from_utf8_lossy(&tail[..end]).trim().to_string()
        } else {
            String::new()
        };
        let model = if model.is_empty() {
            "Unknown drive".to_string()
        } else {
            model
        };
        Some((model, bus_type))
    }

    fn interface_name(bus_type: i32) -> String {
        match bus_type {
            BUS_TYPE_NVME => "NVMe",
            BUS_TYPE_SATA | BUS_TYPE_ATA => "SATA",
            BUS_TYPE_USB => "USB",
            BUS_TYPE_SCSI | BUS_TYPE_SAS | BUS_TYPE_ISCSI => "SCSI",
            _ => "Unknown",
        }
        .to_string()
    }

    fn disk_capacity(handle: Handle) -> Option<u64> {
        let mut buffer = [0u8; 8];
        if ioctl(handle, IOCTL_DISK_GET_LENGTH_INFO, &mut buffer).is_some() {
            Some(u64::from_le_bytes(buffer))
        } else {
            None
        }
    }

    /// The drive's own SMART verdict (`STORAGE_PREDICT_FAILURE`:
    /// a nonzero first DWORD means failure is predicted). A failed
    /// call — or one that returned fewer than the 4 bytes of the
    /// flag — reads as "no prediction", never as a prediction.
    fn predicted_failure(handle: Handle) -> bool {
        let mut buffer = [0u8; 516];
        match ioctl(handle, IOCTL_STORAGE_PREDICT_FAILURE, &mut buffer) {
            Some(returned) if returned >= 4 => {
                u32::from_le_bytes(buffer[0..4].try_into().unwrap_or([0; 4])) != 0
            }
            _ => false,
        }
    }

    /// NVMe SMART/Health log page (LID 02h, 512 bytes) via a
    /// protocol command. The request layout is built (and
    /// unit-tested) by `build_nvme_health_request`; the driver
    /// answers in ReturnStatus @16 and ErrorCode @20, with the log
    /// page itself at `NVME_DATA_OFFSET`.
    fn nvme_health_log(handle: Handle) -> Option<[u8; NVME_LOG_PAGE_LEN]> {
        let mut buffer = build_nvme_health_request();
        ioctl(handle, IOCTL_STORAGE_PROTOCOL_COMMAND, &mut buffer)?;
        let read_u32 = |off: usize| -> u32 {
            u32::from_le_bytes(buffer[off..off + 4].try_into().unwrap_or([0; 4]))
        };
        let status = read_u32(16);
        if status != STORAGE_PROTOCOL_STATUS_SUCCESS {
            eprintln!(
                "diskpilot: NVMe health read failed (ReturnStatus {status}, ErrorCode {})",
                read_u32(20)
            );
            return None;
        }
        let mut log = [0u8; NVME_LOG_PAGE_LEN];
        log.copy_from_slice(&buffer[NVME_DATA_OFFSET..NVME_DATA_OFFSET + NVME_LOG_PAGE_LEN]);
        Some(log)
    }

    /// IDEREGS: the task-file registers of an ATA command.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct IdeRegs {
        features: u8,
        sector_count: u8,
        sector_number: u8,
        cyl_low: u8,
        cyl_high: u8,
        drive_head: u8,
        command: u8,
        reserved: u8,
    }

    /// ATA_PASS_THROUGH_DIRECT (x64 layout, 48 bytes). The data
    /// buffer is a separate allocation, as the "direct" form
    /// requires: the kernel probes the pointed-to buffer itself.
    #[repr(C)]
    struct AtaPassThroughDirect {
        length: u16,
        ata_flags: u16,
        path_id: u8,
        target_id: u8,
        lun: u8,
        reserved_as_uchar: u8,
        data_transfer_length: u32,
        time_out_value: u32,
        reserved_as_ulong: u32,
        data_buffer: *mut c_void,
        previous_task_file: IdeRegs,
        current_task_file: IdeRegs,
    }

    /// ATA SMART READ DATA (command 0xB0, feature 0xD0): the
    /// 512-byte attribute structure the core parser consumes. The
    /// 0x4F/0xC2 cylinder values are the SMART signature the drive
    /// firmware requires before it answers at all.
    fn ata_smart_data(handle: Handle) -> Option<[u8; 512]> {
        let mut data = [0u8; 512];
        let mut request = AtaPassThroughDirect {
            length: std::mem::size_of::<AtaPassThroughDirect>() as u16,
            ata_flags: 0x0002, // ATA_FLAGS_DATA_IN
            path_id: 0,
            target_id: 0,
            lun: 0,
            reserved_as_uchar: 0,
            data_transfer_length: 512,
            time_out_value: 10,
            reserved_as_ulong: 0,
            data_buffer: data.as_mut_ptr() as *mut c_void,
            previous_task_file: IdeRegs::default(),
            current_task_file: IdeRegs {
                features: 0xD0,
                sector_count: 1,
                sector_number: 0,
                cyl_low: 0x4F,
                cyl_high: 0xC2,
                drive_head: 0xA0,
                command: 0xB0,
                reserved: 0,
            },
        };
        let request_size = std::mem::size_of::<AtaPassThroughDirect>();
        let mut returned: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_ATA_PASS_THROUGH_DIRECT,
                &request as *const AtaPassThroughDirect as *const c_void,
                request_size as u32,
                &mut request as *mut AtaPassThroughDirect as *mut c_void,
                request_size as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        Some(data)
    }

    /// Probe one `\\.\PhysicalDriveN`. `None` only when the drive
    /// cannot be opened or described at all — a drive that opens
    /// but will not answer SMART is reported with Unknown vitals
    /// rather than dropped.
    pub fn probe_drive(index: u32) -> Option<DriveHealth> {
        let path = wide_null(Path::new(&format!(r"\\.\PhysicalDrive{index}")));
        // SMART IOCTLs require read/write access (and, for ATA pass
        // -through, an elevated token); a zero-access handle still
        // enumerates and usually answers the descriptor/capacity
        // queries, so the drive degrades to Unknown vitals instead
        // of vanishing when access is limited.
        let guard =
            open_drive(&path, GENERIC_READ | GENERIC_WRITE).or_else(|| open_drive(&path, 0))?;
        let handle = guard.0;
        let (model, bus_type) = device_descriptor(handle)?;
        let interface = interface_name(bus_type);
        let capacity = disk_capacity(handle).unwrap_or(0);
        let predicted = predicted_failure(handle);
        if bus_type == BUS_TYPE_NVME {
            match nvme_health_log(handle) {
                Some(log) => Some(health_from_nvme(
                    &model,
                    &interface,
                    capacity,
                    &parse_nvme_health(&log),
                )),
                None => Some(unknown_health(model, interface, capacity, predicted)),
            }
        } else {
            match ata_smart_data(handle) {
                Some(data) => Some(health_from_ata(
                    &model,
                    &interface,
                    capacity,
                    &parse_ata_attributes(&data),
                    predicted,
                )),
                None => Some(unknown_health(model, interface, capacity, predicted)),
            }
        }
    }
}
