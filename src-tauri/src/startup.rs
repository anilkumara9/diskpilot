//! Startup manager: the programs Windows runs when the user signs
//! in, and reversible enable/disable for each of them.
//!
//! Entries come from four places:
//!
//! - the registry `Run` / `RunOnce` values under HKCU (per-user)
//!   and HKLM (machine-wide) — Windows-only, read through `winreg`;
//! - the per-user and common Startup folders, whose items are the
//!   shortcut / program files themselves.
//!
//! Disabling never deletes anything. A registry value moves to a
//! sibling key DiskPilot owns (`Run-DisabledByDiskPilot`,
//! `RunOnce-DisabledByDiskPilot`, same hive, same value name, raw
//! value data preserved byte for byte), and a Startup-folder file
//! moves into a `Disabled by DiskPilot` subfolder of its Startup
//! folder. Re-enabling moves it back. Disabled entries still
//! enumerate (with `enabled: false`), so what was switched off is
//! always visible and recoverable here — and the moves are just as
//! reversible by hand, in `regedit` or Explorer.
//!
//! The toggle command re-enumerates and matches by the opaque item
//! `id`; a path or key the webview supplies is never acted on. HKLM
//! writes need the app to run as administrator — the registry's
//! access-denied error is mapped to a message that says exactly
//! that.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Subkey (under `Software\Microsoft\Windows\CurrentVersion`) the
/// shell's own autostart also uses; the two bases enumerated.
/// (Registry machinery is Windows-only; on other hosts these are
/// exercised by the unit tests only.)
#[cfg_attr(not(windows), allow(dead_code))]
const RUN_BASES: [&str; 2] = ["Run", "RunOnce"];
/// Suffix of the sibling key disabled registry entries move to.
#[cfg_attr(not(windows), allow(dead_code))]
const DISABLED_KEY_SUFFIX: &str = "-DisabledByDiskPilot";
/// Subfolder of a Startup folder that disabled files move into.
const DISABLED_DIR_NAME: &str = "Disabled by DiskPilot";
/// Registry path of the `CurrentVersion` key the Run keys live
/// under.
#[cfg_attr(not(windows), allow(dead_code))]
const CURRENT_VERSION: &str = r"Software\Microsoft\Windows\CurrentVersion";

/// Where a sign-in entry is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupSource {
    RegistryUser,
    RegistryMachine,
    FolderUser,
    FolderMachine,
}

impl StartupSource {
    /// The wire spelling (matches the serde rename), used in ids.
    fn tag(self) -> &'static str {
        match self {
            StartupSource::RegistryUser => "registryUser",
            StartupSource::RegistryMachine => "registryMachine",
            StartupSource::FolderUser => "folderUser",
            StartupSource::FolderMachine => "folderMachine",
        }
    }
}

/// One program that opens at sign-in, as shown by the Startup page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// Opaque, stable per entry (source + key/folder + name). The
    /// toggle command matches on this after re-enumerating; it is
    /// never parsed back into a location.
    pub id: String,
    /// Registry value name, or the Startup-folder file name.
    pub name: String,
    /// The registry command string, or the folder item's own path.
    pub command: String,
    pub source: StartupSource,
    /// The file the command launches, when it resolves to one.
    pub target_path: Option<String>,
    /// Whether that file is currently on disk.
    pub target_exists: bool,
    /// Allocated bytes of the target file, when it exists as a file.
    pub target_bytes: Option<u64>,
    pub enabled: bool,
}

/// Build an item id: source tag + registry base (when any) + name.
fn make_id(source: StartupSource, base: Option<&str>, name: &str) -> String {
    match base {
        Some(base) => format!("{}|{}|{}", source.tag(), base, name),
        None => format!("{}|{}", source.tag(), name),
    }
}

/// The executable a startup command string launches, exactly as
/// written (environment variables are NOT expanded here — the
/// recorded command is the truth, and `%SystemRoot%\…` stays
/// `%SystemRoot%\…`).
///
/// Rules: a quoted command yields its quoted part; an unquoted one
/// is cut at the first `.exe` / `.com` that ends the path (followed
/// by whitespace, a quote, or the end of the string — an unquoted
/// path may itself contain spaces, `C:\Program Files\…`); otherwise
/// the first whitespace-delimited token is all there is.
pub fn parse_command_target(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        let target = &rest[..end];
        return if target.is_empty() {
            None
        } else {
            Some(target.to_string())
        };
    }
    let lower = command.to_lowercase();
    for ext in [".exe", ".com"] {
        let mut search_from = 0;
        while let Some(found) = lower[search_from..].find(ext) {
            let idx = search_from + found;
            let after = idx + ext.len();
            let boundary = after == command.len()
                || command[after..].starts_with(|c: char| c.is_whitespace() || c == '"');
            if boundary {
                return Some(command[..after].to_string());
            }
            search_from = idx + 1;
        }
    }
    let token = command.split_whitespace().next()?;
    Some(token.to_string())
}

/// Expand `%NAME%` references from the process environment (for
/// probing whether a target exists); unknown names are left as
/// written, so the probe simply misses.
fn expand_env_vars(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        let Some(end_offset) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + end_offset];
        if name.is_empty() {
            break;
        }
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(expanded) => out.push_str(&expanded),
            Err(_) => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &rest[start + 1 + end_offset + 1..];
    }
    out.push_str(rest);
    out
}

/// Resolve a command's target for display: the path as written,
/// whether a file is actually there, and its allocated size when
/// it is. A target that names a directory exists but has no byte
/// figure (a folder is not the program).
fn resolve_target(command: &str) -> (Option<String>, bool, Option<u64>) {
    let Some(raw) = parse_command_target(command) else {
        return (None, false, None);
    };
    let probe = expand_env_vars(&raw);
    match std::fs::metadata(Path::new(&probe)) {
        Ok(md) if md.is_file() => (
            Some(raw.clone()),
            true,
            Some(diskpilot_core::scanner::allocated_size_for_path(Path::new(
                &probe,
            ))),
        ),
        Ok(_) => (Some(raw), true, None),
        Err(_) => (Some(raw), false, None),
    }
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

/// How a toggle acts on one entry. Consumed by the Windows toggle
/// path (and constructed everywhere enumeration runs).
#[cfg_attr(not(windows), allow(dead_code))]
enum Action {
    /// Registry value `item.name` under the `Run`/`RunOnce` base
    /// key of one hive.
    Registry { machine: bool, base: &'static str },
    /// File `item.name` in this (enabled-side) Startup folder.
    Folder { root: PathBuf },
}

#[cfg_attr(not(windows), allow(dead_code))]
struct Entry {
    item: StartupItem,
    action: Action,
}

/// Every startup entry on this machine, enabled and disabled:
/// sorted enabled-first, then by name.
pub fn list_items() -> Vec<StartupItem> {
    let mut items: Vec<StartupItem> =
        enumerate_entries().into_iter().map(|e| e.item).collect();
    items.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    items
}

fn enumerate_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    enumerate_registry_entries(&mut entries);
    for (folder, source) in startup_folders() {
        collect_folder_entries(&folder, source, &mut entries);
    }
    entries
}

/// The two Startup folders (per-user, then common), derived from
/// the environment the way Windows composes them. A missing
/// variable simply removes that source.
fn startup_folders() -> Vec<(PathBuf, StartupSource)> {
    const SUFFIX: &str = r"Microsoft\Windows\Start Menu\Programs\Startup";
    let mut folders = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        folders.push((
            PathBuf::from(appdata).join(SUFFIX),
            StartupSource::FolderUser,
        ));
    }
    if let Some(program_data) = std::env::var_os("ProgramData") {
        folders.push((
            PathBuf::from(program_data).join(SUFFIX),
            StartupSource::FolderMachine,
        ));
    }
    folders
}

/// Collect the entries of one Startup folder: the files directly in
/// it (enabled) and in its `Disabled by DiskPilot` subfolder
/// (disabled). `desktop.ini` is shell bookkeeping, not a program,
/// and is skipped, as are subfolders other than the disabled one.
fn collect_folder_entries(folder: &Path, source: StartupSource, out: &mut Vec<Entry>) {
    collect_folder_side(folder, folder, source, true, out);
    let disabled = folder.join(DISABLED_DIR_NAME);
    collect_folder_side(&disabled, folder, source, false, out);
}

fn collect_folder_side(
    dir: &Path,
    root: &Path,
    source: StartupSource,
    enabled: bool,
    out: &mut Vec<Entry>,
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let command = path.to_string_lossy().into_owned();
        let (target_path, target_exists, target_bytes) = resolve_target(&command);
        out.push(Entry {
            item: StartupItem {
                id: make_id(source, None, &name),
                name,
                command,
                source,
                target_path,
                target_exists,
                target_bytes,
                enabled,
            },
            action: Action::Folder {
                root: root.to_path_buf(),
            },
        });
    }
}

#[cfg(windows)]
fn enumerate_registry_entries(out: &mut Vec<Entry>) {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegType};
    use winreg::types::FromRegValue;
    use winreg::{RegKey, RegValue};

    fn string_value(raw: &RegValue) -> Option<String> {
        match raw.vtype {
            RegType::REG_SZ | RegType::REG_EXPAND_SZ => String::from_reg_value(raw).ok(),
            _ => None,
        }
    }

    for (machine, source) in [
        (false, StartupSource::RegistryUser),
        (true, StartupSource::RegistryMachine),
    ] {
        let hive = RegKey::predef(if machine {
            HKEY_LOCAL_MACHINE
        } else {
            HKEY_CURRENT_USER
        });
        for base in RUN_BASES {
            for enabled in [true, false] {
                let key_name = if enabled {
                    base.to_string()
                } else {
                    format!("{base}{DISABLED_KEY_SUFFIX}")
                };
                let Ok(key) = hive.open_subkey(format!(r"{CURRENT_VERSION}\{key_name}")) else {
                    continue;
                };
                for value in key.enum_values() {
                    let Ok((name, raw)) = value else { continue };
                    let Some(command) = string_value(&raw) else {
                        continue;
                    };
                    let (target_path, target_exists, target_bytes) =
                        resolve_target(&command);
                    out.push(Entry {
                        item: StartupItem {
                            id: make_id(source, Some(base), &name),
                            name,
                            command,
                            source,
                            target_path,
                            target_exists,
                            target_bytes,
                            enabled,
                        },
                        action: Action::Registry { machine, base },
                    });
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn enumerate_registry_entries(_out: &mut Vec<Entry>) {}

// ---------------------------------------------------------------------------
// Toggling
// ---------------------------------------------------------------------------

/// Find an enumerated entry by its opaque id.
#[cfg_attr(not(windows), allow(dead_code))]
fn find_by_id<'a>(entries: &'a [Entry], id: &str) -> Option<&'a Entry> {
    entries.iter().find(|e| e.item.id == id)
}

/// The registry move plan: the `(source key, destination key)`
/// `CurrentVersion`-relative paths a toggle moves the value
/// between. Pure, so the decision is testable without a registry.
#[cfg(any(windows, test))]
fn registry_move_plan(base: &str, disabling: bool) -> (String, String) {
    let enabled_key = format!(r"{CURRENT_VERSION}\{base}");
    let disabled_key = format!(r"{CURRENT_VERSION}\{base}{DISABLED_KEY_SUFFIX}");
    if disabling {
        (enabled_key, disabled_key)
    } else {
        (disabled_key, enabled_key)
    }
}

/// The folder move plan: the `(from, to)` file paths a toggle
/// renames between. Pure path arithmetic.
#[cfg(any(windows, test))]
fn folder_move_plan(root: &Path, name: &str, disabling: bool) -> (PathBuf, PathBuf) {
    let enabled_path = root.join(name);
    let disabled_path = root.join(DISABLED_DIR_NAME).join(name);
    if disabling {
        (enabled_path, disabled_path)
    } else {
        (disabled_path, enabled_path)
    }
}

/// Execute a folder move: refuse when the destination name is
/// already taken (a move never overwrites), create the disabled
/// subfolder when disabling into it.
#[cfg(any(windows, test))]
fn apply_folder_move(root: &Path, name: &str, disabling: bool) -> Result<(), String> {
    let (from, to) = folder_move_plan(root, name, disabling);
    if to.symlink_metadata().is_ok() {
        let where_to = if disabling {
            format!("the {DISABLED_DIR_NAME} folder")
        } else {
            "the Startup folder".to_string()
        };
        return Err(format!(
            "\"{name}\" already exists in {where_to} — remove or rename that copy first, then try again"
        ));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not prepare the folder for \"{name}\": {e}"))?;
    }
    std::fs::rename(&from, &to)
        .map_err(|e| format!("could not move \"{name}\": {e}"))?;
    Ok(())
}

/// Map a registry failure to the message the user should see:
/// machine-wide keys without elevation get the administrator
/// explanation, everything else the OS error in plain words.
#[cfg(windows)]
fn registry_error(machine: bool, action: &str, e: &std::io::Error) -> String {
    if machine && (e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(5))
    {
        "Changing machine-wide startup entries needs DiskPilot running as administrator"
            .to_string()
    } else {
        format!("could not {action} the startup entry in the registry: {e}")
    }
}

/// Execute a registry move: copy the value's raw data to the
/// destination key, then delete it from the source — in that order,
/// so the entry exists somewhere at every moment; if the delete
/// fails, the copy is rolled back so the entry lives in exactly
/// one key.
#[cfg(windows)]
fn apply_registry_move(
    machine: bool,
    base: &str,
    name: &str,
    disabling: bool,
) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    let (src_path, dst_path) = registry_move_plan(base, disabling);
    let hive = RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    });
    let src = hive
        .open_subkey_with_flags(&src_path, KEY_READ | KEY_WRITE)
        .map_err(|e| registry_error(machine, "open", &e))?;
    let (dst, _) = hive
        .create_subkey_with_flags(&dst_path, KEY_READ | KEY_WRITE)
        .map_err(|e| registry_error(machine, "open", &e))?;
    let value = src
        .get_raw_value(name)
        .map_err(|_| "that startup entry no longer exists — refresh the list".to_string())?;
    dst.set_raw_value(name, &value)
        .map_err(|e| registry_error(machine, "copy", &e))?;
    if let Err(e) = src.delete_value(name) {
        let _ = dst.delete_value(name);
        return Err(registry_error(machine, "remove", &e));
    }
    Ok(())
}

/// Enable or disable one startup entry by id. The entry set is
/// re-enumerated and matched by id — the caller's word for where
/// the entry lives is never trusted. Returns the entry with its
/// new state. Toggling to the state the entry is already in is a
/// no-op success.
pub fn set_enabled(id: &str, enabled: bool) -> Result<StartupItem, String> {
    #[cfg(not(windows))]
    {
        let _ = (id, enabled);
        Err("Startup management is Windows-only".to_string())
    }
    #[cfg(windows)]
    {
        let entries = enumerate_entries();
        let entry = find_by_id(&entries, id).ok_or_else(|| {
            "that startup entry no longer exists — refresh the list".to_string()
        })?;
        if entry.item.enabled == enabled {
            return Ok(entry.item.clone());
        }
        // The entry is in the opposite state, so the toggle moves
        // it out of the side it currently lives on.
        let disabling = entry.item.enabled;
        match &entry.action {
            Action::Registry { machine, base } => {
                apply_registry_move(*machine, base, &entry.item.name, disabling)?;
            }
            Action::Folder { root } => {
                apply_folder_move(root, &entry.item.name, disabling)?;
            }
        }
        let mut updated = entry.item.clone();
        updated.enabled = enabled;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_parsing_handles_the_real_command_shapes() {
        // Quoted, with arguments.
        assert_eq!(
            parse_command_target(r#""C:\Program Files\OneDrive\OneDrive.exe" /background"#),
            Some(r"C:\Program Files\OneDrive\OneDrive.exe".to_string())
        );
        // Unquoted path with spaces and arguments: the .exe ends it.
        assert_eq!(
            parse_command_target(r"C:\Program Files\Vendor\app.exe /silent"),
            Some(r"C:\Program Files\Vendor\app.exe".to_string())
        );
        // Unquoted, no arguments.
        assert_eq!(
            parse_command_target(r"C:\Tools\run.exe"),
            Some(r"C:\Tools\run.exe".to_string())
        );
        // ".exe" inside a directory name does not end the path early
        // (it is not followed by a boundary).
        assert_eq!(
            parse_command_target(r"C:\my.exe.tools\helper.exe /x"),
            Some(r"C:\my.exe.tools\helper.exe".to_string())
        );
        // Arguments may mention other executables; the first
        // boundary wins.
        assert_eq!(
            parse_command_target(r"C:\Tools\run.exe --spawn C:\Other\thing.exe"),
            Some(r"C:\Tools\run.exe".to_string())
        );
        // Environment-variable-led paths are left exactly as written.
        assert_eq!(
            parse_command_target(r"%SystemRoot%\system32\ctfmon.exe"),
            Some(r"%SystemRoot%\system32\ctfmon.exe".to_string())
        );
        // A bare program name resolves to its single token.
        assert_eq!(
            parse_command_target("notepad"),
            Some("notepad".to_string())
        );
        assert_eq!(parse_command_target("   "), None);
        assert_eq!(parse_command_target(""), None);
    }

    #[test]
    fn env_expansion_replaces_known_and_keeps_unknown() {
        std::env::set_var("DISKPILOT_TEST_ROOT", r"D:\Apps");
        assert_eq!(
            expand_env_vars(r"%DISKPILOT_TEST_ROOT%\tool.exe"),
            r"D:\Apps\tool.exe"
        );
        assert_eq!(
            expand_env_vars(r"%DISKPILOT_NO_SUCH_VAR%\tool.exe"),
            r"%DISKPILOT_NO_SUCH_VAR%\tool.exe"
        );
        assert_eq!(expand_env_vars(r"C:\plain\path.exe"), r"C:\plain\path.exe");
        std::env::remove_var("DISKPILOT_TEST_ROOT");
    }

    #[test]
    fn ids_are_stable_and_distinguish_source_and_key() {
        let run = make_id(StartupSource::RegistryUser, Some("Run"), "OneDrive");
        assert_eq!(run, "registryUser|Run|OneDrive");
        assert_ne!(
            run,
            make_id(StartupSource::RegistryUser, Some("RunOnce"), "OneDrive"),
            "Run and RunOnce are different entries"
        );
        assert_ne!(
            run,
            make_id(StartupSource::RegistryMachine, Some("Run"), "OneDrive")
        );
        assert_eq!(
            make_id(StartupSource::FolderUser, None, "Notes.lnk"),
            "folderUser|Notes.lnk"
        );
    }

    #[test]
    fn registry_plan_moves_between_run_key_and_its_disabled_sibling() {
        let (src, dst) = registry_move_plan("Run", true);
        assert_eq!(src, r"Software\Microsoft\Windows\CurrentVersion\Run");
        assert_eq!(
            dst,
            r"Software\Microsoft\Windows\CurrentVersion\Run-DisabledByDiskPilot"
        );
        let (src, dst) = registry_move_plan("RunOnce", false);
        assert_eq!(
            src,
            r"Software\Microsoft\Windows\CurrentVersion\RunOnce-DisabledByDiskPilot"
        );
        assert_eq!(dst, r"Software\Microsoft\Windows\CurrentVersion\RunOnce");
    }

    #[test]
    fn folder_plan_and_move_round_trip_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (from, to) = folder_move_plan(root, "App.lnk", true);
        assert_eq!(from, root.join("App.lnk"));
        assert_eq!(to, root.join(DISABLED_DIR_NAME).join("App.lnk"));

        std::fs::write(root.join("App.lnk"), b"shortcut").unwrap();
        apply_folder_move(root, "App.lnk", true).unwrap();
        assert!(!root.join("App.lnk").exists());
        assert_eq!(
            std::fs::read(root.join(DISABLED_DIR_NAME).join("App.lnk")).unwrap(),
            b"shortcut"
        );
        apply_folder_move(root, "App.lnk", false).unwrap();
        assert!(root.join("App.lnk").exists());

        // A same-named file at the destination refuses the move and
        // leaves both copies untouched.
        std::fs::write(root.join(DISABLED_DIR_NAME).join("App.lnk"), b"other").unwrap();
        let err = apply_folder_move(root, "App.lnk", true).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(std::fs::read(root.join("App.lnk")).unwrap(), b"shortcut");
        assert_eq!(
            std::fs::read(root.join(DISABLED_DIR_NAME).join("App.lnk")).unwrap(),
            b"other"
        );
    }

    #[test]
    fn folder_enumeration_lists_enabled_disabled_and_skips_desktop_ini() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Keep.lnk"), b"x").unwrap();
        std::fs::write(root.join("desktop.ini"), b"x").unwrap();
        let disabled = root.join(DISABLED_DIR_NAME);
        std::fs::create_dir_all(&disabled).unwrap();
        std::fs::write(disabled.join("Off.lnk"), b"x").unwrap();

        let mut entries = Vec::new();
        collect_folder_entries(root, StartupSource::FolderUser, &mut entries);
        assert_eq!(entries.len(), 2);
        let keep = find_by_id(&entries, "folderUser|Keep.lnk").unwrap();
        assert!(keep.item.enabled);
        assert!(keep.item.target_exists);
        assert_eq!(
            keep.item.target_bytes,
            Some(diskpilot_core::scanner::allocated_size_for_path(
                &root.join("Keep.lnk")
            )),
            "the byte figure is the core helper's, whatever the host reports"
        );
        let off = find_by_id(&entries, "folderUser|Off.lnk").unwrap();
        assert!(!off.item.enabled);
        assert_eq!(off.item.name, "Off.lnk");
        assert!(find_by_id(&entries, "folderUser|desktop.ini").is_none());
        assert!(find_by_id(&entries, "folderUser|Missing.lnk").is_none());
    }
}
