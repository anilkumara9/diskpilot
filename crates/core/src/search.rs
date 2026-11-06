//! Natural-language file search over the scan index.
//!
//! A query like `pdfs from last week over 5 mb` is parsed by a small,
//! deterministic, closed grammar into [`SearchFilters`], and the parse is
//! echoed back as [`QueryChip`]s so the UI can show exactly how the query
//! was understood. There is no model and no fuzzy matching: anything the
//! grammar does not recognize becomes a plain name term (substring, ANDed),
//! never an error.
//!
//! Grammar (case-insensitive):
//! - **Kinds** — pdf(s), image(s)/photo(s)/picture(s), video(s)/movie(s),
//!   music/song(s)/audio, document(s)/doc(s), spreadsheet(s) → document,
//!   archive(s)/zip(s), app(s)/application(s), screenshot(s),
//!   installer(s)/setup, code/source, text. Multiple kinds are unioned.
//! - **Sizes** — `over|above|more than|bigger than|larger than|>` and
//!   `under|below|less than|smaller than|<` followed by a number and a
//!   unit (b, kb, mb, gb, tb — decimal, so 1 MB = 1,000,000 bytes), as
//!   separate tokens (`over 5 mb`) or attached (`>5mb`). Bounds are
//!   strict: `over 5 MB` means `bytes > 5_000_000`.
//! - **Dates** — today, yesterday, last week (previous 7 days), last
//!   month (previous 30 days), last N days (N ≥ 1), this week (from
//!   Monday 00:00 UTC), this month (from the 1st, 00:00 UTC). All are
//!   lower bounds on mtime except *yesterday*, which is the full
//!   previous UTC day. Day boundaries are UTC to stay deterministic.
//! - **Source** — `from X` where X is a single token; X is matched later
//!   against download provenance (see below), not against the index.
//! - Everything else is a **term**.
//!
//! When a dimension repeats, the first occurrence wins; later phrases of
//! the same dimension are consumed but ignored. Chips come back in
//! canonical order: Kind, Size, Date, Source, then one Terms chip with
//! each term quoted (`"annual" "report"`).
//!
//! **Source-filter split:** [`run_search`] applies kinds/size/date/terms
//! from the index only. The `source` filter is deliberately *not* applied
//! here — provenance lives in an alternate data stream per file, so the
//! shell runs [`run_search`] first and then keeps only hits whose
//! `crate::provenance::download_source` label or host contains the source
//! string (case-insensitive). Candidates are bounded by the 500-hit cap,
//! so the per-file stream reads stay cheap.

use crate::model::Category;
use crate::provenance::DownloadSource;
use crate::scanner::DirIndex;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Which dimension of the query a chip describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChipKind {
    Kind,
    Size,
    Date,
    Source,
    Term,
}

/// One understood piece of a query, for display as a filter chip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryChip {
    pub kind: ChipKind,
    pub label: String,
}

/// The structured form of a parsed query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    /// Canonical kind ids ("pdf", "image", …), unioned.
    pub kinds: Vec<String>,
    /// Exclusive lower size bound in bytes.
    pub min_bytes: Option<u64>,
    /// Exclusive upper size bound in bytes.
    pub max_bytes: Option<u64>,
    /// Inclusive lower mtime bound, ms since the epoch.
    pub modified_after_ms: Option<i64>,
    /// Exclusive upper mtime bound, ms since the epoch.
    pub modified_before_ms: Option<i64>,
    /// The `from X` token as typed, for the shell-side provenance filter.
    pub source: Option<String>,
    /// Lowercased name substrings; every term must match.
    pub terms: Vec<String>,
}

/// A query and its user-visible interpretation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedQuery {
    pub filters: SearchFilters,
    pub chips: Vec<QueryChip>,
}

/// One search result row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub category: Category,
    pub modified_ms: i64,
}

const DAY_MS: i64 = 86_400_000;
/// Maximum hits returned by [`run_search`].
pub const MAX_HITS: usize = 500;

// ------------------------------------------------------------- kinds ----

/// Word form → canonical kind id.
fn kind_id(word: &str) -> Option<&'static str> {
    Some(match word {
        "pdf" | "pdfs" => "pdf",
        "image" | "images" | "photo" | "photos" | "picture" | "pictures" => "image",
        "video" | "videos" | "movie" | "movies" => "video",
        "music" | "song" | "songs" | "audio" => "music",
        "document" | "documents" | "doc" | "docs" | "spreadsheet" | "spreadsheets" => "document",
        "archive" | "archives" | "zip" | "zips" => "archive",
        "app" | "apps" | "application" | "applications" => "app",
        "screenshot" | "screenshots" => "screenshot",
        "installer" | "installers" | "setup" => "installer",
        "code" | "source" => "code",
        "text" => "text",
        _ => return None,
    })
}

/// Canonical kind id → chip display name.
fn kind_label(id: &str) -> &'static str {
    match id {
        "pdf" => "PDFs",
        "image" => "Images",
        "video" => "Videos",
        "music" => "Music",
        "document" => "Documents",
        "archive" => "Archives",
        "app" => "Apps",
        "screenshot" => "Screenshots",
        "installer" => "Installers",
        "code" => "Code",
        "text" => "Text",
        _ => "Files",
    }
}

/// The kinds one file belongs to, from its extension plus two name rules:
/// anything named *screenshot* (or filed under a `Screenshots` folder) is
/// a screenshot, and `setup*`/`install*` executables are installers.
fn kind_ids_for(name: &str, path: &str) -> Vec<&'static str> {
    let lower_name = name.to_lowercase();
    let ext = lower_name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let mut ids: Vec<&'static str> = match ext {
        "pdf" => vec!["pdf"],
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "heic" | "heif" | "svg" | "tif"
        | "tiff" | "ico" => vec!["image"],
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "flv" | "m4v" => vec!["video"],
        "mp3" | "flac" | "wav" | "m4a" | "ogg" | "aac" | "opus" => vec!["music"],
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf"
        | "csv" => vec!["document"],
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "iso" => vec!["archive"],
        "exe" | "msi" | "pkg" | "dmg" | "deb" | "rpm" | "appimage" => vec!["app"],
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "c" | "h" | "cpp" | "hpp"
        | "cs" | "rb" | "php" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "sh" => {
            vec!["code"]
        }
        "txt" | "md" | "log" => vec!["text"],
        _ => vec![],
    };
    let is_screenshot = lower_name.contains("screenshot")
        || path
            .split(['/', '\\'])
            .any(|part| part.eq_ignore_ascii_case("screenshots"));
    if is_screenshot && !ids.contains(&"screenshot") {
        ids.push("screenshot");
    }
    if (ext == "exe" || ext == "msi" || ext == "pkg")
        && (lower_name.starts_with("setup") || lower_name.contains("install"))
        && !ids.contains(&"installer")
    {
        ids.push("installer");
    }
    ids
}

// ------------------------------------------------------------- dates ----

/// Days since the epoch → (year, month, day), Hinnant's civil-from-days.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day) → days since the epoch, Hinnant's days-from-civil.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn day_start(now_ms: i64) -> i64 {
    now_ms.div_euclid(DAY_MS) * DAY_MS
}

/// (after, before) for a date phrase starting at `tokens[i]`, plus how
/// many tokens it spans and its chip label.
fn date_phrase(
    tokens: &[String],
    i: usize,
    now_ms: i64,
) -> Option<(i64, Option<i64>, usize, String)> {
    let word = tokens[i].as_str();
    let next = tokens.get(i + 1).map(String::as_str);
    match (word, next) {
        ("today", _) => Some((day_start(now_ms), None, 1, "Today".to_string())),
        ("yesterday", _) => {
            let start = day_start(now_ms);
            Some((start - DAY_MS, Some(start), 1, "Yesterday".to_string()))
        }
        ("last", Some("week")) => Some((now_ms - 7 * DAY_MS, None, 2, "Last week".to_string())),
        ("last", Some("month")) => Some((now_ms - 30 * DAY_MS, None, 2, "Last month".to_string())),
        ("last", Some(n)) => {
            let days: i64 = n.parse().ok()?;
            if days < 1 || tokens.get(i + 2).map(String::as_str) != Some("days") {
                return None;
            }
            Some((now_ms - days * DAY_MS, None, 3, format!("Last {days} days")))
        }
        ("this", Some("week")) => {
            // Monday 00:00 UTC of the current week. 1970-01-01 was a
            // Thursday; weekday 0 = Sunday.
            let days = now_ms.div_euclid(DAY_MS);
            let weekday = (days + 4).rem_euclid(7);
            let since_monday = (weekday + 6).rem_euclid(7);
            Some((
                (days - since_monday) * DAY_MS,
                None,
                2,
                "This week".to_string(),
            ))
        }
        ("this", Some("month")) => {
            let days = now_ms.div_euclid(DAY_MS);
            let (y, m, _) = civil_from_days(days);
            Some((
                days_from_civil(y, m, 1) * DAY_MS,
                None,
                2,
                "This month".to_string(),
            ))
        }
        _ => None,
    }
}

// ------------------------------------------------------------- sizes ----

fn unit_mult(unit: &str) -> Option<f64> {
    Some(match unit {
        "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return None,
    })
}

/// Split a token like `5mb` / `1.5gb` into (value, unit-multiplier).
fn split_number_unit(token: &str) -> Option<(f64, f64)> {
    let cut = token.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = token.split_at(cut);
    let value: f64 = num.parse().ok()?;
    Some((value, unit_mult(unit)?))
}

/// Parse a size starting at token `i`: either `N` + `unit` as two tokens
/// or a fused `Nunit` token. Returns (bytes, tokens consumed).
fn size_at(tokens: &[String], i: usize) -> Option<(u64, usize)> {
    let token = tokens.get(i)?;
    if let Some((value, mult)) = split_number_unit(token) {
        return Some((((value * mult).round()) as u64, 1));
    }
    let value: f64 = token.parse().ok()?;
    let mult = unit_mult(tokens.get(i + 1)?)?;
    Some((((value * mult).round()) as u64, 2))
}

/// Human size for chip labels, using the largest unit that keeps ≥ 1.
fn format_size(bytes: u64) -> String {
    for (mult, unit) in [(1e12, "TB"), (1e9, "GB"), (1e6, "MB"), (1e3, "KB")] {
        if (bytes as f64) >= mult {
            let v = bytes as f64 / mult;
            if v == v.trunc() {
                return format!("{} {unit}", v as u64);
            }
            return format!("{v:.1} {unit}");
        }
    }
    format!("{bytes} B")
}

// ------------------------------------------------------------- parse ----

/// Parse a natural-language query into filters + display chips.
pub fn parse_query(query: &str, now_ms: i64) -> ParsedQuery {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c| c == '"' || c == ',' || c == ';')
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect();

    let mut filters = SearchFilters::default();
    let mut kind_labels: Vec<&'static str> = Vec::new();
    let mut size_label: Option<String> = None;
    let mut date_label: Option<String> = None;
    let mut have_date = false;
    let mut have_source = false;

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].clone();
        let next = tokens.get(i + 1).cloned();

        // Date phrases (first one wins).
        if let Some((after, before, span, label)) = date_phrase(&tokens, i, now_ms) {
            if !have_date {
                filters.modified_after_ms = Some(after);
                filters.modified_before_ms = before;
                date_label = Some(label);
                have_date = true;
            }
            i += span;
            continue;
        }

        // `from X` — not when X starts a date phrase or a comparison.
        if token == "from" {
            if let Some(x) = &next {
                let x_starts_phrase =
                    date_phrase(&tokens, i + 1, now_ms).is_some() || kind_id(x).is_some();
                if !x_starts_phrase && !have_source {
                    filters.source = Some(x.clone());
                    have_source = true;
                    i += 2;
                    continue;
                }
                if x_starts_phrase {
                    // "from" itself is noise here; let the phrase parse.
                    i += 1;
                    continue;
                }
                // Repeated source: consume the pair, keep the first.
                i += 2;
                continue;
            }
            filters.terms.push(token);
            i += 1;
            continue;
        }

        // Comparators, two-word forms first.
        let two_word = match (token.as_str(), next.as_deref()) {
            ("more" | "bigger" | "larger", Some("than")) => Some((true, 2)),
            ("less" | "smaller", Some("than")) => Some((false, 2)),
            _ => None,
        };
        let one_word = match token.as_str() {
            "over" | "above" | ">" => Some((true, 1)),
            "under" | "below" | "<" => Some((false, 1)),
            _ => None,
        };
        if let Some((is_min, span)) = two_word.or(one_word) {
            if let Some((bytes, used)) = size_at(&tokens, i + span) {
                let label = format!("{} {}", if is_min { ">" } else { "<" }, format_size(bytes));
                if is_min && filters.min_bytes.is_none() {
                    filters.min_bytes = Some(bytes);
                    size_label = Some(match size_label.take() {
                        Some(prev) => format!("{prev} · {label}"),
                        None => label,
                    });
                } else if !is_min && filters.max_bytes.is_none() {
                    filters.max_bytes = Some(bytes);
                    size_label = Some(match size_label.take() {
                        Some(prev) => format!("{prev} · {label}"),
                        None => label,
                    });
                }
                i += span + used;
                continue;
            }
            // Not actually a size: the comparator word becomes a term.
            filters.terms.push(token);
            i += 1;
            continue;
        }

        // Fused comparator: ">5mb" / "<2gb".
        if let Some(rest) = token.strip_prefix('>').or_else(|| token.strip_prefix('<')) {
            let is_min = token.starts_with('>');
            if let Some((value, mult)) = split_number_unit(rest) {
                let bytes = (value * mult).round() as u64;
                if is_min && filters.min_bytes.is_none() {
                    filters.min_bytes = Some(bytes);
                } else if !is_min && filters.max_bytes.is_none() {
                    filters.max_bytes = Some(bytes);
                }
                let label = format!("{} {}", if is_min { ">" } else { "<" }, format_size(bytes));
                size_label = Some(match size_label.take() {
                    Some(prev) => format!("{prev} · {label}"),
                    None => label,
                });
                i += 1;
                continue;
            }
        }

        // Kind words.
        if let Some(id) = kind_id(&token) {
            if !filters.kinds.iter().any(|k| k == id) {
                filters.kinds.push(id.to_string());
                kind_labels.push(kind_label(id));
            }
            i += 1;
            continue;
        }

        // Anything else is a name term.
        filters.terms.push(token);
        i += 1;
    }

    let mut chips = Vec::new();
    if !kind_labels.is_empty() {
        chips.push(QueryChip {
            kind: ChipKind::Kind,
            label: kind_labels.join(", "),
        });
    }
    if let Some(label) = size_label {
        chips.push(QueryChip {
            kind: ChipKind::Size,
            label,
        });
    }
    if let Some(label) = date_label {
        chips.push(QueryChip {
            kind: ChipKind::Date,
            label,
        });
    }
    if let Some(source) = &filters.source {
        chips.push(QueryChip {
            kind: ChipKind::Source,
            label: format!("from {source}"),
        });
    }
    if !filters.terms.is_empty() {
        chips.push(QueryChip {
            kind: ChipKind::Term,
            label: filters
                .terms
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(" "),
        });
    }
    ParsedQuery { filters, chips }
}

// --------------------------------------------------------------- run ----

/// Apply filters to the scan index. The `source` filter is NOT applied
/// here (see the module docs for the split); use
/// [`source_matches`] on the hits with per-file provenance instead.
pub fn run_search(index: &DirIndex, filters: &SearchFilters) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .files
        .iter()
        .filter(|f| {
            if !filters.kinds.is_empty() {
                let ids = kind_ids_for(&f.name, &f.path);
                if !filters.kinds.iter().any(|k| ids.contains(&k.as_str())) {
                    return false;
                }
            }
            if let Some(min) = filters.min_bytes {
                if f.bytes <= min {
                    return false;
                }
            }
            if let Some(max) = filters.max_bytes {
                if f.bytes >= max {
                    return false;
                }
            }
            if let Some(after) = filters.modified_after_ms {
                if f.modified_ms < after {
                    return false;
                }
            }
            if let Some(before) = filters.modified_before_ms {
                if f.modified_ms >= before {
                    return false;
                }
            }
            if !filters.terms.is_empty() {
                let name = f.name.to_lowercase();
                if !filters.terms.iter().all(|t| name.contains(t.as_str())) {
                    return false;
                }
            }
            true
        })
        .map(|f| SearchHit {
            path: f.path.clone(),
            name: f.name.clone(),
            bytes: f.bytes,
            category: f.category,
            modified_ms: f.modified_ms,
        })
        .collect();
    hits.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| a.path.cmp(&b.path))
    });
    hits.truncate(MAX_HITS);
    hits
}

/// Shell-side source check for one file: does its download provenance
/// label or host contain the `from X` string (case-insensitive)?
pub fn source_matches(source: &DownloadSource, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    if source.label.to_lowercase().contains(&needle) {
        return true;
    }
    for url in [source.host_url.as_deref(), source.referrer_url.as_deref()]
        .into_iter()
        .flatten()
    {
        if url.to_lowercase().contains(&needle) {
            return true;
        }
    }
    false
}

/// Convenience for the shell: drop hits whose file (at `path`) has no
/// matching download source.
pub fn filter_hits_by_source(
    hits: Vec<SearchHit>,
    needle: &str,
    source_of: &dyn Fn(&Path) -> Option<DownloadSource>,
) -> Vec<SearchHit> {
    hits.into_iter()
        .filter(|h| match source_of(Path::new(&h.path)) {
            Some(s) => source_matches(&s, needle),
            None => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileEntry;

    // 2023-11-14T22:13:20Z, a Tuesday.
    const NOW: i64 = 1_700_000_000_000;

    fn chip_labels(p: &ParsedQuery) -> Vec<&str> {
        p.chips.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn kind_words_and_plurals() {
        let p = parse_query("pdfs", NOW);
        assert_eq!(p.filters.kinds, vec!["pdf"]);
        assert_eq!(chip_labels(&p), vec!["PDFs"]);
        let p = parse_query("photos and spreadsheets", NOW);
        assert_eq!(p.filters.kinds, vec!["image", "document"]);
        assert_eq!(chip_labels(&p), vec!["Images, Documents", "\"and\""]);
        let p = parse_query("screenshots installers", NOW);
        assert_eq!(p.filters.kinds, vec!["screenshot", "installer"]);
    }

    #[test]
    fn size_forms() {
        let p = parse_query("over 5 mb", NOW);
        assert_eq!(p.filters.min_bytes, Some(5_000_000));
        assert_eq!(chip_labels(&p), vec!["> 5 MB"]);
        let p = parse_query("under 2 gb", NOW);
        assert_eq!(p.filters.max_bytes, Some(2_000_000_000));
        let p = parse_query(">5mb", NOW);
        assert_eq!(p.filters.min_bytes, Some(5_000_000));
        let p = parse_query("more than 1.5 gb", NOW);
        assert_eq!(p.filters.min_bytes, Some(1_500_000_000));
        let p = parse_query("less than 100 kb and over 10 kb", NOW);
        assert_eq!(p.filters.max_bytes, Some(100_000));
        assert_eq!(p.filters.min_bytes, Some(10_000));
        // A bare number+unit with no comparator is NOT a size filter.
        let p = parse_query("5mb", NOW);
        assert_eq!(p.filters.min_bytes, None);
        assert_eq!(p.filters.terms, vec!["5mb"]);
    }

    #[test]
    fn date_phrases() {
        let p = parse_query("today", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(1_699_920_000_000));
        assert_eq!(p.filters.modified_before_ms, None);
        let p = parse_query("yesterday", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(1_699_833_600_000));
        assert_eq!(p.filters.modified_before_ms, Some(1_699_920_000_000));
        let p = parse_query("last week", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(NOW - 7 * DAY_MS));
        let p = parse_query("last month", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(NOW - 30 * DAY_MS));
        let p = parse_query("last 3 days", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(NOW - 3 * DAY_MS));
        assert_eq!(chip_labels(&p), vec!["Last 3 days"]);
        let p = parse_query("this month", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(1_698_796_800_000)); // 2023-11-01
        let p = parse_query("this week", NOW);
        assert_eq!(p.filters.modified_after_ms, Some(1_699_833_600_000)); // Mon 2023-11-13
    }

    #[test]
    fn source_phrase_and_the_from_last_week_trap() {
        let p = parse_query("from github", NOW);
        assert_eq!(p.filters.source.as_deref(), Some("github"));
        assert_eq!(chip_labels(&p), vec!["from github"]);
        // "from last week" must read as a date, not a source named "last".
        let p = parse_query("screenshots from last week", NOW);
        assert_eq!(p.filters.source, None);
        assert_eq!(p.filters.kinds, vec!["screenshot"]);
        assert_eq!(p.filters.modified_after_ms, Some(NOW - 7 * DAY_MS));
    }

    #[test]
    fn full_combo_and_chip_order() {
        let p = parse_query("pdfs from last week over 5 mb annual report", NOW);
        assert_eq!(p.filters.kinds, vec!["pdf"]);
        assert_eq!(p.filters.min_bytes, Some(5_000_000));
        assert_eq!(p.filters.modified_after_ms, Some(NOW - 7 * DAY_MS));
        assert_eq!(p.filters.source, None);
        assert_eq!(p.filters.terms, vec!["annual", "report"]);
        let kinds: Vec<ChipKind> = p.chips.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ChipKind::Kind,
                ChipKind::Size,
                ChipKind::Date,
                ChipKind::Term
            ]
        );
        assert_eq!(p.chips.last().unwrap().label, "\"annual\" \"report\"");

        let p = parse_query("images from canva under 10 mb", NOW);
        let kinds: Vec<ChipKind> = p.chips.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            vec![ChipKind::Kind, ChipKind::Size, ChipKind::Source]
        );
        assert_eq!(p.filters.source.as_deref(), Some("canva"));
    }

    #[test]
    fn unknown_words_become_terms_never_errors() {
        let p = parse_query("flibberty gibbet", NOW);
        assert_eq!(p.filters.terms, vec!["flibberty", "gibbet"]);
        assert!(p.filters.kinds.is_empty());
        let p = parse_query("", NOW);
        assert_eq!(p.chips.len(), 0);
    }

    fn fixture_index() -> DirIndex {
        let mk = |name: &str, bytes: u64, modified_ms: i64| FileEntry {
            path: format!("/root/{name}"),
            name: name.to_string(),
            bytes,
            category: Category::Unknown,
            modified_ms,
        };
        DirIndex {
            files: vec![
                mk("annual report.pdf", 6_000_000, NOW - DAY_MS),
                mk("notes.pdf", 100, NOW - DAY_MS),
                mk("photo.png", 2_000_000, NOW - 2 * DAY_MS),
                mk("Screenshot 2023.png", 500_000, NOW - 3 * DAY_MS),
                mk("Setup.exe", 9_000_000, NOW - 4 * DAY_MS),
                mk("song.mp3", 3_000_000, NOW - 20 * DAY_MS),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn run_search_filters_and_sorts() {
        let index = fixture_index();
        let hits = run_search(&index, &parse_query("pdfs over 5 mb", NOW).filters);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "annual report.pdf");

        let hits = run_search(&index, &parse_query("report", NOW).filters);
        assert_eq!(hits.len(), 1);

        let hits = run_search(&index, &parse_query("screenshots", NOW).filters);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Screenshot 2023.png");

        let hits = run_search(&index, &parse_query("installers", NOW).filters);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Setup.exe");

        // Date window: only files from the last 7 days.
        let hits = run_search(&index, &parse_query("last week", NOW).filters);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0].name, "annual report.pdf", "newest first");

        // Source filter is not applied by run_search.
        let hits = run_search(&index, &parse_query("pdfs from gmail", NOW).filters);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn source_matching_against_provenance() {
        let s = DownloadSource {
            zone_id: Some(3),
            host_url: Some("https://github.com/x/y".to_string()),
            referrer_url: None,
            label: "GitHub".to_string(),
        };
        assert!(source_matches(&s, "github"));
        assert!(source_matches(&s, "git"));
        assert!(!source_matches(&s, "gmail"));
    }
}
