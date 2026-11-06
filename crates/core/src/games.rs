//! Installed games: how much disk Steam and Epic Games libraries
//! actually occupy, read from the launchers' own manifest files on
//! the user's machine. Nothing here needs a scan — the manifests are
//! the launchers' records, parsed read-only at call time.
//!
//! - **Steam** — the install root's `steamapps/libraryfolders.vdf`
//!   lists every library folder; each library's
//!   `steamapps/appmanifest_*.acf` records a game's name, install
//!   directory and `SizeOnDisk`. When `SizeOnDisk` is missing (older
//!   or partial manifests), the install directory is measured
//!   directly with allocated sizes, the same byte rule as a scan.
//! - **Epic Games** — each installed game has a JSON manifest in
//!   `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`
//!   with `DisplayName`, `InstallLocation` and `InstallSize`.
//!
//! Malformed or partial manifests are skipped, never fatal: a
//! library folder that yields no games simply yields no library in
//! the report, and a machine with neither launcher gets an empty
//! report rather than an error.

use crate::scanner::allocated_size_for_path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------- types ----

/// One installed game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub name: String,
    /// Bytes on disk (allocated): the launcher's own recorded size
    /// where it keeps one, otherwise a direct measurement.
    pub bytes: u64,
    /// The game's install directory.
    pub path: String,
    /// Steam app id / Epic catalog item id (manifest file stem when
    /// the manifest carries no id).
    pub app_id: String,
}

/// One library folder (Steam) or install folder group (Epic — the
/// launcher keeps no library list, so its games are grouped by the
/// folder that contains them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLibrary {
    /// `"Steam"` or `"Epic"`.
    pub source: String,
    pub path: String,
    /// Games in this library, biggest first.
    pub games: Vec<Game>,
    /// Sum of the games' bytes.
    pub bytes: u64,
}

/// Every installed game found on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesReport {
    /// Libraries with at least one game, biggest first.
    pub libraries: Vec<GameLibrary>,
    pub total_bytes: u64,
    pub total_games: usize,
}

// --------------------------------------------------- VDF mini-parser ----

#[derive(Debug, PartialEq)]
enum VdfToken {
    Str(String),
    Open,
    Close,
}

/// Tokenize the subset of Valve's KeyValues text format that library
/// and app manifests use: quoted strings (with `\\` and `\"`
/// escapes), bare words, `{` / `}`, and `//` line comments.
fn tokenize_vdf(text: &str) -> Vec<VdfToken> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '{' {
            tokens.push(VdfToken::Open);
            i += 1;
        } else if c == '}' {
            tokens.push(VdfToken::Close);
            i += 1;
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    // \\ → \, \" → ", any other escape keeps the char.
                    s.push(chars[i + 1]);
                    i += 2;
                } else {
                    s.push(chars[i]);
                    i += 1;
                }
            }
            i += 1; // closing quote (or end of input)
            tokens.push(VdfToken::Str(s));
        } else {
            // Bare word: up to whitespace or a brace.
            let mut s = String::new();
            while i < chars.len()
                && !chars[i].is_whitespace()
                && chars[i] != '{'
                && chars[i] != '}'
            {
                s.push(chars[i]);
                i += 1;
            }
            if !s.is_empty() {
                tokens.push(VdfToken::Str(s));
            }
        }
    }
    tokens
}

/// Every value whose key is `key` (case-insensitive) in a KeyValues
/// document, in document order.
fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let tokens = tokenize_vdf(text);
    let mut out = Vec::new();
    for pair in tokens.windows(2) {
        if let (VdfToken::Str(k), VdfToken::Str(v)) = (&pair[0], &pair[1]) {
            if k.eq_ignore_ascii_case(key) {
                out.push(v.clone());
            }
        }
    }
    out
}

fn vdf_first(text: &str, key: &str) -> Option<String> {
    vdf_values(text, key).into_iter().next()
}

/// All library paths recorded in a `libraryfolders.vdf`: the modern
/// format nests one `"path"` per numbered library entry. Returned
/// unescaped (`D:\\SteamLibrary`), in file order, duplicates kept
/// (the caller dedupes against the install root).
pub fn parse_library_paths(vdf: &str) -> Vec<String> {
    vdf_values(vdf, "path")
}

/// The fields DiskPilot reads from one `appmanifest_*.acf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
    /// `SizeOnDisk` as recorded by Steam; `None` when the manifest
    /// predates the field or leaves it unparseable.
    pub size_on_disk: Option<u64>,
}

/// Parse one Steam app manifest. `None` when the name or install
/// directory is missing — a manifest without them cannot be placed.
pub fn parse_app_manifest(acf: &str) -> Option<AppManifest> {
    let name = vdf_first(acf, "name")?;
    let install_dir = vdf_first(acf, "installdir")?;
    Some(AppManifest {
        app_id: vdf_first(acf, "appid").unwrap_or_default(),
        name,
        install_dir,
        size_on_disk: vdf_first(acf, "SizeOnDisk").and_then(|v| v.parse().ok()),
    })
}

/// The fields DiskPilot reads from one Epic `.item` manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicManifest {
    pub name: String,
    pub install_location: String,
    pub install_size: u64,
    pub catalog_item_id: Option<String>,
}

/// Parse one Epic Games manifest (JSON). `None` when the display
/// name or install location is missing or the JSON does not parse.
pub fn parse_epic_manifest(json: &str) -> Option<EpicManifest> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let name = value.get("DisplayName")?.as_str()?.to_string();
    let install_location = value.get("InstallLocation")?.as_str()?.to_string();
    Some(EpicManifest {
        name,
        install_location,
        install_size: value
            .get("InstallSize")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        catalog_item_id: value
            .get("CatalogItemId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

// --------------------------------------------------------- Steam --------

/// Candidate Steam install roots, most likely first.
fn steam_root_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            if !base.is_empty() {
                out.push(PathBuf::from(base).join("Steam"));
            }
        }
    }
    out.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    out
}

/// The Steam install root: the first candidate that actually has a
/// `steamapps` folder.
pub fn detect_steam_root() -> Option<PathBuf> {
    steam_root_candidates()
        .into_iter()
        .find(|root| root.join("steamapps").is_dir())
}

/// Every Steam library folder: the install root plus each path in
/// its `libraryfolders.vdf`, deduplicated (case-insensitively, as
/// Windows paths), keeping only folders that exist.
pub fn steam_library_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![root.to_path_buf()];
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        paths.extend(parse_library_paths(&vdf).into_iter().map(PathBuf::from));
    }
    let mut seen: Vec<String> = Vec::new();
    paths
        .into_iter()
        .filter(|p| {
            let key = p
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase();
            if seen.contains(&key) || !p.is_dir() {
                return false;
            }
            seen.push(key);
            true
        })
        .collect()
}

/// Entry budget for the fallback measurement walk, so a pathological
/// install tree cannot keep the command busy indefinitely.
const MEASURE_ENTRY_BUDGET: usize = 250_000;

/// Allocated size of a directory tree, symlink-free, giving up
/// (returning what it has) after [`MEASURE_ENTRY_BUDGET`] entries.
fn measure_capped(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut budget = MEASURE_ENTRY_BUDGET;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            if budget == 0 {
                return total;
            }
            budget -= 1;
            let Ok(md) = entry.metadata() else { continue };
            // entry.metadata() follows links for the final component;
            // symlink_metadata is the honest check for skipping them.
            let Ok(smd) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if smd.file_type().is_symlink() {
                continue;
            }
            if md.is_dir() {
                stack.push(entry.path());
            } else if md.is_file() {
                total = total.saturating_add(allocated_size_for_path(&entry.path()));
            }
        }
    }
    total
}

/// All games recorded in one Steam library folder.
pub fn steam_games_in_library(library: &Path) -> Vec<Game> {
    let steamapps = library.join("steamapps");
    let Ok(read) = std::fs::read_dir(&steamapps) else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for entry in read.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Some(manifest) = parse_app_manifest(&text) else {
            continue;
        };
        let install_path = steamapps.join("common").join(&manifest.install_dir);
        // A recorded size of 0 means "not recorded" for an installed
        // game; fall through to measuring the folder either way when
        // the field is absent.
        let bytes = match manifest.size_on_disk {
            Some(size) if size > 0 => size,
            _ => measure_capped(&install_path),
        };
        let app_id = if manifest.app_id.is_empty() {
            file_name
                .trim_start_matches("appmanifest_")
                .trim_end_matches(".acf")
                .to_string()
        } else {
            manifest.app_id
        };
        games.push(Game {
            name: manifest.name,
            bytes,
            path: install_path.to_string_lossy().into_owned(),
            app_id,
        });
    }
    sort_games(&mut games);
    games
}

// ---------------------------------------------------------- Epic --------

/// The Epic Games Launcher manifests folder, if it exists.
pub fn epic_manifests_dir() -> Option<PathBuf> {
    let base = std::env::var_os("ProgramData")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let dir = base
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    dir.is_dir().then_some(dir)
}

/// All games recorded in an Epic manifests folder, grouped by the
/// folder that contains their install location.
pub fn epic_libraries(manifests_dir: &Path) -> Vec<GameLibrary> {
    let Ok(read) = std::fs::read_dir(manifests_dir) else {
        return Vec::new();
    };
    let mut groups: BTreeMap<String, Vec<Game>> = BTreeMap::new();
    for entry in read.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Some(manifest) = parse_epic_manifest(&text) else {
            continue;
        };
        let location = Path::new(&manifest.install_location);
        let group = location
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| manifest.install_location.clone());
        let app_id = manifest.catalog_item_id.clone().unwrap_or_else(|| {
            entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        groups.entry(group).or_default().push(Game {
            name: manifest.name,
            bytes: manifest.install_size,
            path: manifest.install_location,
            app_id,
        });
    }
    groups
        .into_iter()
        .map(|(path, mut games)| {
            sort_games(&mut games);
            let bytes = games.iter().map(|g| g.bytes).sum();
            GameLibrary {
                source: "Epic".to_string(),
                path,
                games,
                bytes,
            }
        })
        .collect()
}

// --------------------------------------------------------- report -------

fn sort_games(games: &mut [Game]) {
    games.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
}

/// Read every installed game on this machine from the launchers'
/// manifests. No launcher installed → an empty report, not an error.
pub fn games_report() -> GamesReport {
    let mut libraries: Vec<GameLibrary> = Vec::new();
    if let Some(root) = detect_steam_root() {
        for library_path in steam_library_paths(&root) {
            let games = steam_games_in_library(&library_path);
            if games.is_empty() {
                continue;
            }
            let bytes = games.iter().map(|g| g.bytes).sum();
            libraries.push(GameLibrary {
                source: "Steam".to_string(),
                path: library_path.to_string_lossy().into_owned(),
                games,
                bytes,
            });
        }
    }
    if let Some(dir) = epic_manifests_dir() {
        libraries.extend(epic_libraries(&dir));
    }
    libraries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    let total_bytes = libraries.iter().map(|l| l.bytes).sum();
    let total_games = libraries.iter().map(|l| l.games.len()).sum();
    GamesReport {
        libraries,
        total_bytes,
        total_games,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARYFOLDERS: &str = r#"
"libraryfolders"
{
    "0"
    {
        "path"      "C:\\Program Files (x86)\\Steam"
        "label"     ""
    }
    "1"
    {
        "path"      "D:\\SteamLibrary"
        "label"     ""
    }
    "2"
    {
        "path"      "E:\\Games\\Steam Library"
    }
}
"#;

    #[test]
    fn vdf_extracts_all_library_paths_unescaped() {
        let paths = parse_library_paths(LIBRARYFOLDERS);
        assert_eq!(
            paths,
            vec![
                r"C:\Program Files (x86)\Steam",
                r"D:\SteamLibrary",
                r"E:\Games\Steam Library",
            ]
        );
    }

    const APPMANIFEST: &str = r#"
"AppState"
{
    "appid"     "1245620"
    "name"      "ELDEN RING"
    "installdir"    "ELDEN RING"
    "SizeOnDisk"    "71360913408"
    "LastUpdated"   "1730000000"
}
"#;

    #[test]
    fn acf_extracts_fields() {
        let m = parse_app_manifest(APPMANIFEST).unwrap();
        assert_eq!(m.app_id, "1245620");
        assert_eq!(m.name, "ELDEN RING");
        assert_eq!(m.install_dir, "ELDEN RING");
        assert_eq!(m.size_on_disk, Some(71_360_913_408));
    }

    #[test]
    fn acf_without_size_on_disk() {
        let text = r#""AppState" { "appid" "570" "name" "Dota 2" "installdir" "dota 2 beta" }"#;
        let m = parse_app_manifest(text).unwrap();
        assert_eq!(m.name, "Dota 2");
        assert_eq!(m.size_on_disk, None);
    }

    #[test]
    fn acf_without_name_or_installdir_is_none() {
        assert!(parse_app_manifest(r#""AppState" { "appid" "570" }"#).is_none());
        assert!(parse_app_manifest("").is_none());
    }

    #[test]
    fn epic_manifest_parses() {
        let json = r#"{
            "DisplayName": "Fortnite",
            "InstallLocation": "D:\\Epic Games\\Fortnite",
            "InstallSize": 104857600,
            "CatalogItemId": "abc123"
        }"#;
        let m = parse_epic_manifest(json).unwrap();
        assert_eq!(m.name, "Fortnite");
        assert_eq!(m.install_location, r"D:\Epic Games\Fortnite");
        assert_eq!(m.install_size, 104_857_600);
        assert_eq!(m.catalog_item_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn garbage_input_never_panics() {
        assert!(parse_library_paths("").is_empty());
        assert!(parse_library_paths("{{{ \"path\" ").is_empty());
        assert!(parse_library_paths("\u{1f600} not vdf at all").is_empty());
        assert!(parse_app_manifest("}{ garbage \"\"\"").is_none());
        assert!(parse_epic_manifest("not json").is_none());
        assert!(parse_epic_manifest("{}").is_none());
        assert!(parse_epic_manifest(r#"{"DisplayName": 42}"#).is_none());
    }

    #[test]
    fn steam_libraries_dedupe_and_require_existence() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        let steamapps = root.join("steamapps");
        std::fs::create_dir_all(&steamapps).unwrap();
        // The VDF lists the root itself (typical entry "0") plus a
        // library that does not exist on this machine.
        std::fs::write(
            steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                root.to_string_lossy().replace('\\', "\\\\"),
                dir.path().join("Missing").to_string_lossy()
            ),
        )
        .unwrap();
        let libraries = steam_library_paths(&root);
        assert_eq!(libraries, vec![root.clone()]);
    }

    #[test]
    fn steam_games_read_manifests_and_measure_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let steamapps = dir.path().join("steamapps");
        let game_dir = steamapps.join("common").join("Tiny Game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("game.bin"), vec![7u8; 5_000]).unwrap();
        // One manifest with a recorded size, one without (fallback
        // measures the install folder).
        std::fs::write(
            steamapps.join("appmanifest_100.acf"),
            "\"AppState\" { \"appid\" \"100\" \"name\" \"Recorded\" \"installdir\" \"Tiny Game\" \"SizeOnDisk\" \"12345\" }",
        )
        .unwrap();
        std::fs::write(
            steamapps.join("appmanifest_200.acf"),
            "\"AppState\" { \"appid\" \"200\" \"name\" \"Measured\" \"installdir\" \"Tiny Game\" }",
        )
        .unwrap();
        std::fs::write(steamapps.join("appmanifest_bad.acf"), "garbage").unwrap();
        let games = steam_games_in_library(dir.path());
        assert_eq!(games.len(), 2);
        let recorded = games.iter().find(|g| g.name == "Recorded").unwrap();
        assert_eq!(recorded.bytes, 12_345);
        assert_eq!(recorded.app_id, "100");
        let measured = games.iter().find(|g| g.name == "Measured").unwrap();
        assert!(measured.bytes >= 5_000, "allocated size of the 5000-byte file");
        assert_eq!(measured.app_id, "200");
    }
}
