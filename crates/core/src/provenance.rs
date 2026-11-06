//! Download provenance.
//!
//! On Windows, browsers, mail clients and chat apps stamp every file they
//! download with a `Zone.Identifier` alternate data stream — a tiny INI
//! document (`[ZoneTransfer]`, `ZoneId`, `ReferrerUrl`, `HostUrl`) attached
//! to the file without touching its contents. This module parses that
//! stream, normalizes the origin into a human label ("Gmail", "GitHub",
//! "example.com"), and groups the downloads under a folder by source.
//!
//! The stream only exists on Windows/NTFS, so [`download_source`] returns
//! `None` on other platforms and [`scan_provenance`] honestly returns an
//! empty list there; the parser itself is platform-independent and fully
//! unit-tested with fixture strings.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Where one file came from, normalized for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSource {
    /// Zone id from the stream (3 = Internet) when present.
    pub zone_id: Option<u32>,
    /// URL of the host that served the file, when recorded.
    pub host_url: Option<String>,
    /// URL of the page that linked to the download, when recorded.
    pub referrer_url: Option<String>,
    /// Display label derived from the host URL (see module docs).
    pub label: String,
}

/// One file that carries a download source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceFile {
    pub path: String,
    pub bytes: u64,
    pub modified_ms: i64,
}

/// All downloads under a folder that share one source label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceGroup {
    pub label: String,
    /// Bare host the label came from, when a URL was recorded.
    pub host: Option<String>,
    /// Files, newest first.
    pub files: Vec<ProvenanceFile>,
    pub total_bytes: u64,
}

/// A recent file in a downloads folder, with its source when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub modified_ms: i64,
    pub source: Option<DownloadSource>,
}

/// Known host → friendly label mappings.
fn known_label(host: &str) -> Option<&'static str> {
    let h = host.to_lowercase();
    let bare = h.strip_prefix("www.").unwrap_or(&h);
    Some(match bare {
        "mail.google.com" => "Gmail",
        "drive.google.com" => "Google Drive",
        "chatgpt.com" => "ChatGPT",
        "github.com" => "GitHub",
        "app.slack.com" | "slack.com" => "Slack",
        "web.whatsapp.com" => "WhatsApp",
        "x.com" | "twitter.com" => "X",
        "canva.com" => "Canva",
        "teams.microsoft.com" => "Microsoft Teams",
        "dropbox.com" => "Dropbox",
        _ => return None,
    })
}

/// Extract the bare host from a URL-ish string: strips the scheme, any
/// userinfo/port/path, and a leading `www.`. Returns `None` when nothing
/// host-like remains.
fn host_of(url: &str) -> Option<String> {
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let authority = after_scheme.split(['/', '?', '#']).next()?.trim();
    let host_port = authority.rsplit('@').next()?;
    // Keep IPv6 literals intact; otherwise drop a trailing :port.
    let host = if host_port.starts_with('[') {
        host_port
    } else {
        host_port.split(':').next()?
    };
    let host = host.trim().trim_end_matches('.').to_lowercase();
    if host.is_empty() || !host.contains('.') && host != "localhost" {
        return None;
    }
    let bare = host.strip_prefix("www.").unwrap_or(&host);
    Some(bare.to_string())
}

fn label_for(host_url: Option<&str>, referrer_url: Option<&str>) -> (String, Option<String>) {
    for url in [host_url, referrer_url].into_iter().flatten() {
        if let Some(host) = host_of(url) {
            if let Some(label) = known_label(&host) {
                return (label.to_string(), Some(host));
            }
            return (host.clone(), Some(host));
        }
    }
    ("Unknown source".to_string(), None)
}

/// Parse the text of a `Zone.Identifier` stream. Returns `None` when the
/// content has no `[ZoneTransfer]` section (keys are matched
/// case-insensitively and surrounding whitespace is ignored).
pub fn parse_zone_identifier(content: &str) -> Option<DownloadSource> {
    let mut in_section = false;
    let mut saw_section = false;
    let mut zone_id = None;
    let mut host_url = None;
    let mut referrer_url = None;
    for raw in content.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[zonetransfer]");
            saw_section |= in_section;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim().to_lowercase().as_str() {
            "zoneid" => zone_id = value.parse::<u32>().ok(),
            "hosturl" if !value.is_empty() => host_url = Some(value),
            "referrerurl" if !value.is_empty() => referrer_url = Some(value),
            _ => {}
        }
    }
    if !saw_section {
        return None;
    }
    let (label, _) = label_for(host_url.as_deref(), referrer_url.as_deref());
    Some(DownloadSource {
        zone_id,
        host_url,
        referrer_url,
        label,
    })
}

/// Read and parse the `Zone.Identifier` stream of one file. Windows only;
/// returns `None` elsewhere, and `None` whenever the file carries no
/// stream. Only the alternate stream is read — never the file's contents.
pub fn download_source(path: &Path) -> Option<DownloadSource> {
    #[cfg(windows)]
    {
        let ads = format!("{}:Zone.Identifier", path.display());
        let bytes = std::fs::read(&ads).ok()?;
        // Streams are written as ANSI or UTF-16LE depending on the writer.
        let content = decode_stream(&bytes);
        parse_zone_identifier(&content)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

#[cfg(windows)]
fn decode_stream(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn mtime_ms(md: &std::fs::Metadata) -> i64 {
    match md.modified() {
        Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_millis().min(i64::MAX as u128) as i64,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// Walk `dir` recursively (following the duplicate finder's conventions
/// for junk directories) and group every file that carries a download
/// source. Files within a group are newest first; groups are ordered by
/// total bytes, largest first.
pub fn scan_provenance(dir: &Path) -> Vec<ProvenanceGroup> {
    let files = crate::duplicates::collect_files(dir);
    let found: Vec<(DownloadSource, ProvenanceFile)> = files
        .into_par_iter()
        .filter_map(|path| {
            let source = download_source(&path)?;
            let md = std::fs::metadata(&path).ok()?;
            Some((
                source,
                ProvenanceFile {
                    path: path.to_string_lossy().to_string(),
                    // Allocated bytes, like every user-facing figure (see
                    // the byte-accounting rule in scanner/mod.rs docs).
                    bytes: crate::scanner::allocated_size_for_path(&path),
                    modified_ms: mtime_ms(&md),
                },
            ))
        })
        .collect();
    let mut by_label: std::collections::HashMap<String, (Option<String>, Vec<ProvenanceFile>)> =
        std::collections::HashMap::new();
    for (source, file) in found {
        let host = host_of(source.host_url.as_deref().unwrap_or(""))
            .or_else(|| source.referrer_url.as_deref().and_then(host_of));
        by_label
            .entry(source.label)
            .or_insert_with(|| (host, Vec::new()))
            .1
            .push(file);
    }
    let mut groups: Vec<ProvenanceGroup> = by_label
        .into_iter()
        .map(|(label, (host, mut files))| {
            files.sort_by_key(|f| std::cmp::Reverse(f.modified_ms));
            let total_bytes = files.iter().map(|f| f.bytes).sum();
            ProvenanceGroup {
                label,
                host,
                files,
                total_bytes,
            }
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.total_bytes));
    groups
}

/// The most recent files at the TOP LEVEL of `dir` (not recursive),
/// newest first, each with its download source attached when it has one.
pub fn recent_downloads(dir: &Path, limit: usize) -> Vec<RecentFile> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<RecentFile> = read
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let md = std::fs::metadata(&path).ok()?;
            if !md.is_file() {
                return None;
            }
            Some(RecentFile {
                name: entry.file_name().to_string_lossy().to_string(),
                source: download_source(&path),
                path: path.to_string_lossy().to_string(),
                // Allocated bytes, like every user-facing figure (see
                // the byte-accounting rule in scanner/mod.rs docs).
                bytes: crate::scanner::allocated_size_for_path(&path),
                modified_ms: mtime_ms(&md),
            })
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.modified_ms));
    files.truncate(limit);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gmail_download() {
        let content = "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://mail.google.com/mail/u/0/\r\nHostUrl=https://mail.google.com/mail/u/0/\r\n";
        let s = parse_zone_identifier(content).expect("gmail stream parses");
        assert_eq!(s.zone_id, Some(3));
        assert_eq!(s.label, "Gmail");
        assert!(s.host_url.as_deref().unwrap().contains("mail.google.com"));
    }

    #[test]
    fn parses_domain_only_host() {
        let content =
            "[ZoneTransfer]\nZoneId=3\nHostUrl=https://www.example.com/files/report.zip\n";
        let s = parse_zone_identifier(content).expect("domain stream parses");
        assert_eq!(s.label, "example.com");
    }

    #[test]
    fn parses_known_hosts_and_referrer_fallback() {
        let gh = parse_zone_identifier("[ZoneTransfer]\nHostUrl=https://github.com/o/r/releases\n")
            .unwrap();
        assert_eq!(gh.label, "GitHub");
        let referrer_only =
            parse_zone_identifier("[ZoneTransfer]\nReferrerUrl=https://chatgpt.com/c/abc\n")
                .unwrap();
        assert_eq!(referrer_only.label, "ChatGPT");
        assert_eq!(referrer_only.host_url, None);
    }

    #[test]
    fn zone_id_only_stream_is_unknown_source() {
        let s = parse_zone_identifier("[ZoneTransfer]\nZoneId=3\n").expect("zone-only parses");
        assert_eq!(s.zone_id, Some(3));
        assert_eq!(s.label, "Unknown source");
        assert_eq!(s.host_url, None);
    }

    #[test]
    fn garbage_and_missing_section_return_none() {
        assert!(parse_zone_identifier("").is_none());
        assert!(parse_zone_identifier("random text\nHostUrl=https://x.com/\n").is_none());
        assert!(parse_zone_identifier("[SomethingElse]\nHostUrl=https://x.com/\n").is_none());
    }

    #[test]
    fn keys_are_case_and_whitespace_tolerant() {
        let content = "  [zonetransfer] \n  ZONEID = 3 \n hosturl = https://app.slack.com/file \n";
        let s = parse_zone_identifier(content).expect("slack stream parses");
        assert_eq!(s.label, "Slack");
        assert_eq!(s.zone_id, Some(3));
    }

    #[test]
    fn host_extraction_edges() {
        assert_eq!(
            host_of("https://user@www.Example.COM:8443/a/b?x=1").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("not a url"), None);
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn recent_downloads_top_level_only_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"bb").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("c.txt"), b"ccc").unwrap();
        let recent = recent_downloads(dir.path(), 10);
        assert_eq!(recent.len(), 2, "subdirectory contents are excluded");
        assert!(recent.iter().all(|f| f.source.is_none()));
        assert_eq!(recent_downloads(dir.path(), 1).len(), 1);
    }

    #[test]
    fn scan_provenance_is_empty_without_streams() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plain.txt"), b"data").unwrap();
        assert!(scan_provenance(dir.path()).is_empty());
    }

    /// Byte-accounting regression: a small file must report its
    /// ALLOCATED bytes (the scanner helper's value) — not its logical
    /// length — on every surface that displays a size: Downloads
    /// recents, exact-duplicate rows + wasted math, and look-alike
    /// rows + wasted math. Expected values always come from
    /// `allocated_size_for_path`, never from hard-coded block sizes, so
    /// the test is portable across filesystems.
    #[test]
    fn small_file_reports_allocated_bytes_on_every_surface() {
        use crate::duplicates::{find_duplicates, find_similar_images};
        use crate::scanner::allocated_size_for_path;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Two identical tiny PNGs (logical size is a few hundred bytes)
        // plus one small text file.
        let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            image::ImageBuffer::from_fn(8, 8, |_, _| image::Rgba([120, 30, 200, 255]));
        img.save(root.join("a.png")).unwrap();
        img.save(root.join("b.png")).unwrap();
        std::fs::write(root.join("notes.txt"), vec![b'x'; 200]).unwrap();

        let alloc_a = allocated_size_for_path(&root.join("a.png"));
        let alloc_b = allocated_size_for_path(&root.join("b.png"));
        let alloc_txt = allocated_size_for_path(&root.join("notes.txt"));
        let logical_txt = std::fs::metadata(root.join("notes.txt")).unwrap().len();
        assert_eq!(alloc_a, alloc_b, "identical copies allocate alike");
        if alloc_txt > logical_txt {
            // The normal case on a block filesystem — the whole point
            // of the rule (a 200-byte file really occupies a block).
            assert!(alloc_txt > logical_txt);
        }

        // Surface 1: Downloads recents.
        let recent = recent_downloads(root, 10);
        let txt = recent.iter().find(|f| f.name == "notes.txt").unwrap();
        assert_eq!(txt.bytes, alloc_txt, "recents must show allocated bytes");
        let png = recent.iter().find(|f| f.name == "a.png").unwrap();
        assert_eq!(png.bytes, alloc_a, "recents must show allocated bytes");

        // Surface 2: exact duplicates — per-file bytes and wasted math.
        let groups = find_duplicates(root);
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert!(group.files.iter().all(|f| f.bytes == alloc_a));
        assert_eq!(
            group.wasted_bytes(),
            alloc_a,
            "wasted = the removed copy's allocated bytes"
        );

        // Surface 3: look-alike images — per-file bytes and wasted math.
        let similar = find_similar_images(root);
        assert_eq!(similar.len(), 1);
        let group = &similar[0];
        assert_eq!(group.files.len(), 2);
        assert!(group.files.iter().all(|f| f.bytes == alloc_a));
        assert_eq!(
            group.wasted_bytes, alloc_a,
            "wasted = sum of allocated minus the keeper's allocated"
        );
    }
}
