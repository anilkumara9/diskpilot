//! CSV export for power users: the scan's file list and category
//! totals as plain RFC-4180 CSV (CRLF line endings, fields quoted
//! when they contain a comma, quote, or newline, internal quotes
//! doubled). Columns are exactly the fields the model types carry —
//! nothing is invented for the export's sake. The shell writes the
//! returned string to the file the user picks.

use std::collections::HashMap;

use crate::model::{Category, FileEntry};

/// Quote one field per RFC 4180: wrap in double quotes when the
/// value contains a comma, a double quote, or a line break, doubling
/// any internal double quotes.
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// The category's stable machine name (its serde spelling, e.g.
/// `buildArtifacts`) — the value a spreadsheet or script can join
/// against, rather than the human label.
fn category_name(category: Category) -> String {
    match serde_json::to_string(&category) {
        Ok(s) => s.trim_matches('"').to_string(),
        Err(_) => format!("{category:?}"),
    }
}

/// The scan's files as CSV:
/// `path,name,bytes,modified_ms,category` — one row per entry, in
/// the order given. (`FileEntry` carries no separate allocated-size
/// field, so no such column exists.) Empty input yields the header
/// only.
pub fn files_to_csv(entries: &[FileEntry]) -> String {
    let mut out = String::from("path,name,bytes,modified_ms,category\r\n");
    for e in entries {
        out.push_str(&csv_field(&e.path));
        out.push(',');
        out.push_str(&csv_field(&e.name));
        out.push(',');
        out.push_str(&e.bytes.to_string());
        out.push(',');
        out.push_str(&e.modified_ms.to_string());
        out.push(',');
        out.push_str(&csv_field(&category_name(e.category)));
        out.push_str("\r\n");
    }
    out
}

/// Category totals as CSV: `category,bytes` — one row per category
/// present in the summary's totals map, sorted by bytes descending
/// (ties by category name) so two exports of the same scan are
/// byte-identical. Empty input yields the header only.
pub fn categories_to_csv(category_totals: &HashMap<Category, u64>) -> String {
    let mut rows: Vec<(String, u64)> = category_totals
        .iter()
        .map(|(c, b)| (category_name(*c), *b))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out = String::from("category,bytes\r\n");
    for (name, bytes) in rows {
        out.push_str(&csv_field(&name));
        out.push(',');
        out.push_str(&bytes.to_string());
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, name: &str, bytes: u64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: name.to_string(),
            bytes,
            category: Category::Cache,
            modified_ms: 1_700_000_000_000,
            ocr_text: None,
        }
    }

    #[test]
    fn empty_input_is_header_only() {
        assert_eq!(
            files_to_csv(&[]),
            "path,name,bytes,modified_ms,category\r\n"
        );
        assert_eq!(categories_to_csv(&HashMap::new()), "category,bytes\r\n");
    }

    #[test]
    fn plain_rows_are_unquoted_with_crlf() {
        let rows = files_to_csv(&[entry("C:/Users/x/a.bin", "a.bin", 42)]);
        assert_eq!(
            rows,
            "path,name,bytes,modified_ms,category\r\n\
             C:/Users/x/a.bin,a.bin,42,1700000000000,cache\r\n"
        );
    }

    #[test]
    fn quoting_edge_cases() {
        // Comma in the name, quote in the path, newline in a name.
        let rows = files_to_csv(&[
            entry("C:/dir \"quoted\"/f.bin", "report, final.bin", 1),
            entry("C:/x/y.bin", "line\nbreak.bin", 2),
        ]);
        let expected = "path,name,bytes,modified_ms,category\r\n\
            \"C:/dir \"\"quoted\"\"/f.bin\",\"report, final.bin\",1,1700000000000,cache\r\n\
            C:/x/y.bin,\"line\nbreak.bin\",2,1700000000000,cache\r\n";
        assert_eq!(rows, expected);
    }

    #[test]
    fn categories_sorted_by_bytes_then_name() {
        let mut totals = HashMap::new();
        totals.insert(Category::Cache, 500);
        totals.insert(Category::Logs, 900);
        totals.insert(Category::Downloads, 900);
        assert_eq!(
            categories_to_csv(&totals),
            "category,bytes\r\n\
             downloads,900\r\n\
             logs,900\r\n\
             cache,500\r\n"
        );
    }

    #[test]
    fn category_names_use_serde_spelling() {
        assert_eq!(category_name(Category::BuildArtifacts), "buildArtifacts");
        assert_eq!(category_name(Category::SystemFiles), "systemFiles");
        assert_eq!(category_name(Category::Unknown), "unknown");
    }
}
