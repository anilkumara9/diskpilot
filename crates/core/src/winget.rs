//! Parser for `winget upgrade` table output.
//!
//! Winget prints a fixed-column table whose column widths vary with
//! the longest value, so rows are sliced by the column offsets of the
//! header line — never split on whitespace (package names contain
//! spaces). All offsets are CHAR offsets end to end (the header
//! search and the row slicer agree), so a multibyte character before
//! a column word can never shift the columns. Winget also prints
//! progress/spinner garbage before the table on a captured stream,
//! so parsing starts by *searching* for the header line rather than
//! assuming line zero. When the header is localized and the English
//! column words are absent, the dashes separator under the header
//! still draws one run per column: the column spans are derived
//! from those runs instead, and every parsed id is validated before
//! it can reach a consumer, so a mis-sliced row drops out rather
//! than surfacing as a garbage package id.
//!
//! Pure text in, structured rows out. Running winget, validating the
//! packages it names, and acting on the result are the shell's job.

use serde::{Deserialize, Serialize};

/// One row of `winget upgrade`: an installed package with a newer
/// version available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub name: String,
    pub id: String,
    pub current_version: String,
    pub available_version: String,
}

/// Column start offsets (in chars) derived from the header line.
struct Columns {
    name: usize,
    id: usize,
    version: usize,
    available: usize,
    /// Start of the optional trailing `Source` column; the Available
    /// column ends here when present.
    end: Option<usize>,
}

/// Find `needle` in `line` at or after CHAR offset `from`,
/// returning its char offset. (`str::find` yields byte offsets;
/// feeding those to the char-based slicer shifted every column
/// whenever a multibyte character preceded a column word.)
fn find_from(line: &str, needle: &str, from: usize) -> Option<usize> {
    let byte_start = line
        .char_indices()
        .nth(from)
        .map(|(b, _)| b)
        .unwrap_or(line.len());
    line[byte_start..]
        .find(needle)
        .map(|b| line[..byte_start + b].chars().count())
}

fn find_columns(line: &str) -> Option<Columns> {
    let name = find_from(line, "Name", 0)?;
    let id = find_from(line, "Id", name + 4)?;
    let version = find_from(line, "Version", id + 2)?;
    let available = find_from(line, "Available", version + 7)?;
    let end = find_from(line, "Source", available + 9);
    Some(Columns {
        name,
        id,
        version,
        available,
        end,
    })
}

/// Derive column spans from a dashes separator line: one run of
/// `-` per column, runs separated by spaces, each run starting
/// where its column starts. Returns None unless the line is purely
/// dashes and spaces with at least four runs (Name, Id, Version,
/// Available) — a single unbroken rule carries no column
/// information and is not enough.
fn columns_from_separator(line: &str) -> Option<Columns> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c == '-' || c == ' ') {
        return None;
    }
    let mut starts = Vec::new();
    let mut in_run = false;
    for (i, c) in line.chars().enumerate() {
        match c {
            '-' if !in_run => {
                starts.push(i);
                in_run = true;
            }
            '-' => {}
            _ => in_run = false,
        }
    }
    if starts.len() < 4 {
        return None;
    }
    Some(Columns {
        name: starts[0],
        id: starts[1],
        version: starts[2],
        available: starts[3],
        end: starts.get(4).copied(),
    })
}

/// Slice `line` by char offsets `[start, next)` and trim. Char-based
/// (not byte-based) so a non-ASCII package name can never split a
/// UTF-8 boundary and panic.
fn slice_chars(line: &str, start: usize, next: Option<usize>) -> String {
    let chars: Vec<char> = line.chars().collect();
    if start >= chars.len() {
        return String::new();
    }
    let end = next.unwrap_or(chars.len()).min(chars.len());
    if end <= start {
        return String::new();
    }
    chars[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

/// Whether a line is the dashes separator winget prints under the
/// header.
fn is_separator(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().all(|c| c == '-')
}

/// Whether a line ends the data table: the "N upgrades available."
/// footer, or the lead-in of a following section ("The following
/// packages are pinned…" and friends), whose rows use a different
/// shape and must not be read as upgrades.
fn is_table_end(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("The following")
        || (t.contains("upgrade") && t.contains("available.") && !t.contains("Available version"))
        || t.ends_with("upgrades available.")
        || t.ends_with("upgrade available.")
}

/// Parse the full captured stdout of `winget upgrade`.
///
/// Returns an empty vec for the "no upgrades" outputs (which carry
/// no table at all) and for output with no recognizable header.
pub fn parse_winget_upgrade(output: &str) -> Vec<AvailableUpdate> {
    // Captured streams mix \r (progress redraws) into the text; \r is
    // a line boundary for our purposes. \r\n then yields an empty
    // line between rows, which the row loop simply skips.
    let normalized = output.replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    // Find the header, tolerating spinner/progress garbage before
    // it. `start` is the index of the first line after the header
    // (or after the separator, in the localized fallback).
    let mut table: Option<(Columns, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.contains("Name") && line.contains("Available") {
            if let Some(cols) = find_columns(line) {
                table = Some((cols, i + 1));
                break;
            }
        }
    }
    // Localized fallback: the header above the separator names the
    // columns in another language, but the separator's dash runs
    // still mark where each column starts.
    if table.is_none() {
        for (i, line) in lines.iter().enumerate() {
            if let Some(cols) = columns_from_separator(line) {
                table = Some((cols, i + 1));
                break;
            }
        }
    }
    let Some((cols, start)) = table else {
        return Vec::new();
    };

    let mut updates = Vec::new();
    for line in &lines[start..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_separator(line) {
            continue;
        }
        if is_table_end(line) {
            break;
        }
        let name = slice_chars(line, cols.name, Some(cols.id));
        let id = slice_chars(line, cols.id, Some(cols.version));
        let current_version = slice_chars(line, cols.version, Some(cols.available));
        let available_version = slice_chars(line, cols.available, cols.end);
        if name.is_empty() || id.is_empty() || available_version.is_empty() {
            continue;
        }
        // Never hand a consumer a garbage id: a row whose id is not
        // shaped like a package id, or that winget itself truncated
        // with an ellipsis, is dropped here — the shell should not
        // have to re-filter what the parser already knows is bad.
        if !is_valid_package_id(&id) || id.contains('\u{2026}') {
            continue;
        }
        updates.push(AvailableUpdate {
            name,
            id,
            current_version,
            available_version,
        });
    }
    updates
}

/// Whether `id` is shaped like a winget package id: 2..=128 chars
/// from `[A-Za-z0-9._+-]`, with no leading or trailing dot. The shell
/// re-checks every id through this before handing it back to winget
/// as an argument, so a malformed table row can never become a
/// command-line surprise.
pub fn is_valid_package_id(id: &str) -> bool {
    let len = id.chars().count();
    if !(2..=128).contains(&len) {
        return false;
    }
    if id.starts_with('.') || id.ends_with('.') {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Name                          Id                                Version           Available         Source
--------------------------------------------------------------------------------------------------------
7-Zip                         7zip.7zip                         24.09             25.01             winget
Git                           Git.Git                           2.47.1            2.48.1            winget
Microsoft Visual Studio Code  Microsoft.VisualStudioCode        1.96.2            1.97.0            winget
Node.js                       OpenJS.NodeJS                     Unknown           22.14.0           winget

4 upgrades available.
";

    #[test]
    fn parses_multiword_names_and_unknown_version() {
        let rows = parse_winget_upgrade(SAMPLE);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].name, "7-Zip");
        assert_eq!(rows[0].id, "7zip.7zip");
        assert_eq!(rows[0].current_version, "24.09");
        assert_eq!(rows[0].available_version, "25.01");
        assert_eq!(rows[2].name, "Microsoft Visual Studio Code");
        assert_eq!(rows[2].id, "Microsoft.VisualStudioCode");
        assert_eq!(rows[3].current_version, "Unknown");
        assert_eq!(rows[3].available_version, "22.14.0");
    }

    #[test]
    fn tolerates_spinner_garbage_and_crlf_before_header() {
        let mut noisy = String::from("- \\ | / ▒▒▒▒▒▒▒▒▒▒  10%\r");
        noisy.push_str(&SAMPLE.replace('\n', "\r\n"));
        let rows = parse_winget_upgrade(&noisy);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].id, "Git.Git");
    }

    #[test]
    fn no_upgrades_outputs_are_empty() {
        assert!(parse_winget_upgrade(
            "No available upgrade found.\nNo newer package versions are available from the configured sources.\n"
        )
        .is_empty());
        assert!(parse_winget_upgrade("No newer version available.\n").is_empty());
        assert!(parse_winget_upgrade("").is_empty());
        assert!(parse_winget_upgrade("random noise\nwith no table\n").is_empty());
    }

    #[test]
    fn stops_at_following_section() {
        let out = "\
Name        Id             Version  Available  Source
-----------------------------------------------------
App One     Contoso.App    1.0      2.0        winget

1 upgrade available.

The following packages are pinned:
Name        Id             Version
----------------------------------
Pinned App  Contoso.Pin    3.0
";
        let rows = parse_winget_upgrade(out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "Contoso.App");
    }

    #[test]
    fn table_without_source_column() {
        let out = "\
Name     Id           Version  Available
----------------------------------------
Solo     Solo.App     1.0      1.1
";
        let rows = parse_winget_upgrade(out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].available_version, "1.1");
    }

    #[test]
    fn column_offsets_are_char_based() {
        // Two multibyte block chars precede the header words. Byte
        // offsets would put Name at 7; the slicer indexes by char,
        // so the columns must come back as char offsets.
        let cols = find_columns("▒▒ Name  Id  Version  Available").expect("columns");
        assert_eq!(cols.name, 3);
        assert_eq!(cols.id, 9);
        assert_eq!(cols.version, 13);
        assert_eq!(cols.available, 22);
        assert_eq!(cols.end, None);
    }

    #[test]
    fn truncated_and_invalid_ids_are_dropped() {
        let out = format!(
            "{:<10}{:<16}{:<9}{:<11}{}\n{}\n{:<10}{:<16}{:<9}{:<11}{}\n{:<10}{:<16}{:<9}{:<11}{}\n{:<10}{:<16}{:<9}{:<11}{}\n",
            "Name",
            "Id",
            "Version",
            "Available",
            "Source",
            "-".repeat(60),
            "Good App",
            "Good.App",
            "1.0",
            "2.0",
            "winget",
            "Truncated",
            "Microsoft.Vis\u{2026}",
            "1.0",
            "2.0",
            "winget",
            "Bad Id",
            "has space",
            "1.0",
            "2.0",
            "winget",
        );
        let rows = parse_winget_upgrade(&out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "Good.App");
        assert_eq!(rows[0].available_version, "2.0");
    }

    #[test]
    fn localized_header_falls_back_to_separator_spans() {
        // German winget: the header names its columns Name / Id /
        // Version / Verfügbar / Quelle, so the English search finds
        // nothing; the separator's dash runs carry the columns.
        let out = format!(
            "{:<12}{:<14}{:<9}{:<11}{}\n{:<12}{:<14}{:<9}{:<11}{}\n{:<12}{:<14}{:<9}{:<11}{}\n\n1 Upgrade verfügbar.\n",
            "Name",
            "Id",
            "Version",
            "Verfügbar",
            "Quelle",
            "----------",
            "------------",
            "-------",
            "---------",
            "------",
            "7-Zip",
            "7zip.7zip",
            "24.09",
            "25.01",
            "winget",
        );
        let rows = parse_winget_upgrade(&out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "7-Zip");
        assert_eq!(rows[0].id, "7zip.7zip");
        assert_eq!(rows[0].current_version, "24.09");
        assert_eq!(rows[0].available_version, "25.01");
    }

    #[test]
    fn package_id_validation() {
        assert!(is_valid_package_id("7zip.7zip"));
        assert!(is_valid_package_id("Microsoft.VisualStudioCode"));
        assert!(is_valid_package_id("OpenJS.NodeJS.LTS"));
        assert!(is_valid_package_id("AB"));
        assert!(!is_valid_package_id("a"));
        assert!(!is_valid_package_id(".leading.dot"));
        assert!(!is_valid_package_id("trailing.dot."));
        assert!(!is_valid_package_id("has space.id"));
        assert!(!is_valid_package_id("semi;colon.id"));
        assert!(!is_valid_package_id("slash/id"));
        assert!(!is_valid_package_id(&"x".repeat(129)));
        assert!(is_valid_package_id(&"x".repeat(128)));
    }
}
