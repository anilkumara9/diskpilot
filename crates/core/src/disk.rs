//! Disk space information for a path's volume.

use crate::model::DiskInfo;
use std::path::Path;

pub fn disk_info_for(path: &Path) -> DiskInfo {
    #[cfg(windows)]
    {
        if let Some(info) = windows_disk_info(path) {
            return info;
        }
    }
    #[cfg(unix)]
    {
        if let Some(info) = unix_disk_info(path) {
            return info;
        }
    }
    DiskInfo {
        total_bytes: 0,
        used_bytes: 0,
        available_bytes: 0,
        volume_name: path.to_string_lossy().to_string(),
    }
}

#[cfg(unix)]
fn unix_disk_info(path: &Path) -> Option<DiskInfo> {
    // Minimal statvfs through libc-free approach: parse `df` is fragile, so
    // use the std-only fallback of reading /proc/mounts is overkill. Instead
    // call statvfs via an extern declaration against libc symbols, which are
    // always linked on Linux.
    #[repr(C)]
    struct Statvfs {
        f_bsize: u64,
        f_frsize: u64,
        f_blocks: u64,
        f_bfree: u64,
        f_bavail: u64,
        f_files: u64,
        f_ffree: u64,
        f_favail: u64,
        f_fsid: u64,
        f_flag: u64,
        f_namemax: u64,
        f_spare: [i32; 6],
    }
    extern "C" {
        fn statvfs(path: *const std::ffi::c_char, buf: *mut Statvfs) -> i32;
    }
    let cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut buf = Statvfs {
        f_bsize: 0,
        f_frsize: 0,
        f_blocks: 0,
        f_bfree: 0,
        f_bavail: 0,
        f_files: 0,
        f_ffree: 0,
        f_favail: 0,
        f_fsid: 0,
        f_flag: 0,
        f_namemax: 0,
        f_spare: [0; 6],
    };
    let rc = unsafe { statvfs(cstr.as_ptr(), &mut buf) };
    if rc != 0 {
        return None;
    }
    let unit = if buf.f_frsize > 0 {
        buf.f_frsize
    } else {
        buf.f_bsize
    };
    let total = buf.f_blocks.saturating_mul(unit);
    let avail = buf.f_bavail.saturating_mul(unit);
    let free = buf.f_bfree.saturating_mul(unit);
    let used = total.saturating_sub(free);
    Some(DiskInfo {
        total_bytes: total,
        used_bytes: used,
        available_bytes: avail,
        volume_name: path.to_string_lossy().to_string(),
    })
}

#[cfg(windows)]
fn windows_disk_info(path: &Path) -> Option<DiskInfo> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut avail: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok == 0 {
        return None;
    }
    Some(DiskInfo {
        total_bytes: total,
        used_bytes: total.saturating_sub(free),
        available_bytes: avail,
        volume_name: path.to_string_lossy().to_string(),
    })
}
