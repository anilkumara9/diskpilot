//! Installed applications and leftover data.
//! On Windows the installed list comes from the uninstall registry keys; the
//! data size is measured in the per-user application-data folders whose names
//! match the app. AppData folders with no matching installed app are reported
//! as leftovers — for review only, never removed automatically.

use crate::model::AppEntry;
use crate::scanner::measure_path;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RawInstalledApp {
    pub name: String,
    pub publisher: String,
    pub install_location: Option<PathBuf>,
    pub estimated_size_bytes: u64,
}

fn normalise_name(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn name_matches(folder: &str, app_name: &str) -> bool {
    let f = normalise_name(folder);
    let a = normalise_name(app_name);
    if f.is_empty() || a.is_empty() {
        return false;
    }
    f == a || f.contains(&a) || a.contains(&f)
}

/// Read installed apps from the Windows uninstall registry. On other
/// platforms this returns an empty list: there is no equivalent registry to
/// read, and the leftover analysis still runs from the data folders alone.
pub fn read_installed_apps() -> Vec<RawInstalledApp> {
    #[cfg(windows)]
    {
        read_installed_apps_windows()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn read_installed_apps_windows() -> Vec<RawInstalledApp> {
    // Registry access through reg.exe is avoided; the Tauri shell does not
    // grant shell access. The engine reads the registry via the Win32 API
    // through raw declarations to keep the dependency set small.
    windows_registry_uninstall_entries()
}

#[cfg(windows)]
fn windows_registry_uninstall_entries() -> Vec<RawInstalledApp> {
    // Minimal RegOpenKeyExW / RegEnumKeyExW / RegQueryValueExW walk over the
    // three uninstall roots. Implemented with raw extern declarations.
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    type Hkey = isize;
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(hkey: Hkey, sub: *const u16, opts: u32, sam: u32, out: *mut Hkey) -> i32;
        fn RegEnumKeyExW(
            hkey: Hkey,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            class: *mut u16,
            class_len: *mut u32,
            ft: *mut u64,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: Hkey,
            value: *const u16,
            reserved: *mut u32,
            typ: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegCloseKey(hkey: Hkey) -> i32;
    }
    const HKLM: Hkey = 0x80000002isize;
    const HKCU: Hkey = 0x80000001isize;
    const KEY_READ: u32 = 0x20019;
    const ERROR_SUCCESS: i32 = 0;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let read_string = |key: Hkey, name: &str| -> Option<String> {
        let mut len: u32 = 0;
        let w = wide(name);
        let rc = unsafe {
            RegQueryValueExW(
                key,
                w.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let rc = unsafe {
            RegQueryValueExW(
                key,
                w.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let units: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        Some(OsString::from_wide(&units).to_string_lossy().to_string())
    };

    let roots: [(Hkey, &str); 3] = [
        (HKLM, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (
            HKLM,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (HKCU, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];
    let mut apps = Vec::new();
    for (root, sub) in roots {
        let mut key: Hkey = 0;
        let w = wide(sub);
        if unsafe { RegOpenKeyExW(root, w.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
            continue;
        }
        let mut index = 0u32;
        loop {
            let mut name_buf = vec![0u16; 512];
            let mut name_len = 512u32;
            let rc = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if rc != ERROR_SUCCESS {
                break;
            }
            index += 1;
            let sub_name = OsString::from_wide(&name_buf[..name_len as usize])
                .to_string_lossy()
                .to_string();
            let sub_path = format!("{}\\{}", sub, sub_name);
            let mut sub_key: Hkey = 0;
            let sw = wide(&sub_path);
            if unsafe { RegOpenKeyExW(root, sw.as_ptr(), 0, KEY_READ, &mut sub_key) }
                != ERROR_SUCCESS
            {
                continue;
            }
            let display = read_string(sub_key, "DisplayName");
            if let Some(name) = display {
                if !name.trim().is_empty() {
                    let publisher = read_string(sub_key, "Publisher").unwrap_or_default();
                    let location = read_string(sub_key, "InstallLocation")
                        .filter(|s| !s.trim().is_empty())
                        .map(PathBuf::from);
                    apps.push(RawInstalledApp {
                        name,
                        publisher,
                        install_location: location,
                        estimated_size_bytes: 0,
                    });
                }
            }
            unsafe { RegCloseKey(sub_key) };
        }
        unsafe { RegCloseKey(key) };
    }
    apps
}

/// Build the full application list, measuring data folders and flagging
/// leftovers. `data_roots` are the Local and Roaming application-data roots.
pub fn build_app_entries(installed: &[RawInstalledApp], data_roots: &[PathBuf]) -> Vec<AppEntry> {
    let mut entries: Vec<AppEntry> = Vec::new();
    let mut claimed_folders: Vec<PathBuf> = Vec::new();

    for app in installed {
        let mut data_bytes = 0u64;
        let mut data_paths: Vec<String> = Vec::new();
        for root in data_roots {
            if let Ok(read) = std::fs::read_dir(root) {
                for entry in read.flatten() {
                    let folder = entry.file_name().to_string_lossy().to_string();
                    if name_matches(&folder, &app.name) {
                        let p = entry.path();
                        data_bytes += measure_path(&p);
                        data_paths.push(p.to_string_lossy().to_string());
                        claimed_folders.push(p);
                    }
                }
            }
        }
        let install_bytes = match &app.install_location {
            Some(loc) if loc.exists() => measure_path(loc),
            _ => app.estimated_size_bytes,
        };
        entries.push(AppEntry {
            name: app.name.clone(),
            publisher: app.publisher.clone(),
            install_bytes,
            data_bytes,
            data_paths,
            leftover: false,
        });
    }

    // Leftovers: data folders not claimed by any installed app, skipping
    // well-known shared system folders.
    const SHARED: &[&str] = &["microsoft", "packages", "temp", "programs", "common"];
    for root in data_roots {
        if let Ok(read) = std::fs::read_dir(root) {
            for entry in read.flatten() {
                let p = entry.path();
                if !p.is_dir() || claimed_folders.contains(&p) {
                    continue;
                }
                let folder = entry.file_name().to_string_lossy().to_string();
                if SHARED.contains(&normalise_name(&folder).as_str()) {
                    continue;
                }
                let bytes = measure_path(&p);
                if bytes == 0 {
                    continue;
                }
                // Only call it a leftover when an installed list exists to
                // compare against; otherwise every folder would be flagged.
                if installed.is_empty() {
                    continue;
                }
                entries.push(AppEntry {
                    name: folder,
                    publisher: String::new(),
                    install_bytes: 0,
                    data_bytes: bytes,
                    data_paths: vec![p.to_string_lossy().to_string()],
                    leftover: true,
                });
            }
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.install_bytes + e.data_bytes));
    entries
}

pub fn get_applications() -> Vec<AppEntry> {
    let installed = read_installed_apps();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(local) = dirs::data_local_dir() {
        roots.push(local);
    }
    if let Some(roaming) = dirs::config_dir() {
        roots.push(roaming);
    }
    build_app_entries(&installed, &roots)
}

pub fn measure_app_data(app_name: &str, data_roots: &[PathBuf]) -> (u64, Vec<String>) {
    let mut bytes = 0u64;
    let mut paths = Vec::new();
    for root in data_roots {
        if let Ok(read) = std::fs::read_dir(root) {
            for entry in read.flatten() {
                let folder = entry.file_name().to_string_lossy().to_string();
                if name_matches(&folder, app_name) {
                    bytes += measure_path(&entry.path());
                    paths.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
    }
    (bytes, paths)
}

pub fn data_roots_for_home(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("AppData").join("Local"),
        home.join("AppData").join("Roaming"),
    ]
}
