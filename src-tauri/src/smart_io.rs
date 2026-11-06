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

    use super::unknown_health;

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
    /// buffers. Returns whether the call succeeded.
    fn ioctl(handle: Handle, code: u32, buffer: &mut [u8]) -> bool {
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
        ok != 0
    }

    /// `(model, bus type)` from the storage device descriptor. The
    /// descriptor is a fixed header followed by NUL-terminated
    /// strings at absolute offsets; the product id sits at the
    /// offset stored in bytes 16..20, the bus type at 28..32.
    fn device_descriptor(handle: Handle) -> Option<(String, i32)> {
        // STORAGE_PROPERTY_QUERY: PropertyId 0
        // (StorageDeviceProperty), QueryType 0 (PropertyStandardQuery).
        let mut buffer = [0u8; 1024];
        if !ioctl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &mut buffer) {
            return None;
        }
        let read_u32 = |off: usize| -> u32 {
            u32::from_le_bytes(buffer[off..off + 4].try_into().unwrap_or([0; 4]))
        };
        let product_offset = read_u32(16) as usize;
        let bus_type = i32::from_le_bytes(buffer[28..32].try_into().unwrap_or([0; 4]));
        let model = if product_offset > 0 && product_offset < buffer.len() {
            let tail = &buffer[product_offset..];
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
        if ioctl(handle, IOCTL_DISK_GET_LENGTH_INFO, &mut buffer) {
            Some(u64::from_le_bytes(buffer))
        } else {
            None
        }
    }

    /// The drive's own SMART verdict (`STORAGE_PREDICT_FAILURE`:
    /// a nonzero first DWORD means failure is predicted). A failed
    /// call reads as "no prediction", never as a prediction.
    fn predicted_failure(handle: Handle) -> bool {
        let mut buffer = [0u8; 516];
        if !ioctl(handle, IOCTL_STORAGE_PREDICT_FAILURE, &mut buffer) {
            return false;
        }
        u32::from_le_bytes(buffer[0..4].try_into().unwrap_or([0; 4])) != 0
    }

    /// NVMe SMART/Health log page (LID 02h, 512 bytes) via a
    /// protocol command. Buffer layout: the 60-byte
    /// `STORAGE_PROTOCOL_COMMAND` header, the 64-byte NVMe admin
    /// command at offset 60, and the log page at offset 128.
    fn nvme_health_log(handle: Handle) -> Option<[u8; 512]> {
        const HEADER_LEN: usize = 60;
        const COMMAND_LEN: usize = 64;
        const DATA_OFFSET: usize = 128;
        let mut buffer = vec![0u8; DATA_OFFSET + 512];
        let put_u32 = |buffer: &mut [u8], off: usize, value: u32| {
            buffer[off..off + 4].copy_from_slice(&value.to_le_bytes());
        };
        put_u32(&mut buffer, 0, 1); // Version
        put_u32(&mut buffer, 4, HEADER_LEN as u32); // Length (header only)
        put_u32(&mut buffer, 8, 3); // ProtocolType = ProtocolTypeNvme
        put_u32(&mut buffer, 16, COMMAND_LEN as u32); // CommandLength
        put_u32(&mut buffer, 28, DATA_OFFSET as u32); // DataFromDeviceBufferOffset
        put_u32(&mut buffer, 36, 512); // DataFromDeviceTransferLength
        put_u32(&mut buffer, 40, 10); // TimeOutValue (seconds)
                                      // NVMe admin command (Get Log Page) at offset 60:
        put_u32(&mut buffer, 60, 0x02); // CDW0: opcode 02h
        put_u32(&mut buffer, 64, 0xFFFF_FFFF); // NSID: controller-wide log
                                               // CDW10: LID 02h in the low byte, NUMD (dwords - 1 = 127)
                                               // in bits 16..=31 — 512 bytes total.
        put_u32(&mut buffer, 100, 0x02 | (127 << 16));
        if !ioctl(handle, IOCTL_STORAGE_PROTOCOL_COMMAND, &mut buffer) {
            return None;
        }
        // ReturnStatus (offset 52): 0 = success.
        if u32::from_le_bytes(buffer[52..56].try_into().unwrap_or([0; 4])) != 0 {
            return None;
        }
        let mut log = [0u8; 512];
        log.copy_from_slice(&buffer[DATA_OFFSET..DATA_OFFSET + 512]);
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
