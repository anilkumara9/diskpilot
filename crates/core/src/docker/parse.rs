//! Parser for `docker system df -v` human-readable output.

use crate::model::*;

/// Parse a human size like "1.234GB", "56.78MB", "123kB", "0B" into bytes
/// (decimal units, matching Docker's display).
pub fn parse_human_size(s: &str) -> u64 {
    let t = s.trim();
    if t.is_empty() || t == "-" {
        return 0;
    }
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let (num_part, unit_part) = t.split_at(split);
    let value: f64 = num_part.trim().replace(',', "").parse().unwrap_or(0.0);
    let mult: f64 = match unit_part.trim().to_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KIB" => 1024.0,
        "MIB" => 1024.0_f64.powi(2),
        "GIB" => 1024.0_f64.powi(3),
        "TIB" => 1024.0_f64.powi(4),
        _ => 1.0,
    };
    (value * mult).round() as u64
}

/// Parse "123.4MB (45%)" returning the byte amount before the parenthesis.
pub fn parse_size_with_percent(s: &str) -> u64 {
    let head = s.split('(').next().unwrap_or(s);
    parse_human_size(head)
}

fn split_columns(line: &str) -> Vec<String> {
    // Docker tables separate columns with runs of 2+ spaces.
    let mut cols = Vec::new();
    let mut cur = String::new();
    let mut spaces = 0;
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            spaces += 1;
            if spaces >= 2 && !cur.trim().is_empty() {
                cols.push(cur.trim().to_string());
                cur = String::new();
                spaces = 0;
            }
            continue;
        }
        if spaces > 0 && !cur.is_empty() {
            cur.push(' ');
        }
        spaces = 0;
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        cols.push(cur.trim().to_string());
    }
    cols
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Images,
    Containers,
    Volumes,
    BuildCache,
}

/// Parse the full `docker system df -v` output into an overview.
pub fn parse_system_df_v(text: &str) -> DockerOverview {
    let mut overview = DockerOverview {
        installed: true,
        running: true,
        detail: "Parsed from docker system df -v".to_string(),
        ..Default::default()
    };
    let mut section = Section::None;
    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("images space usage") {
            section = Section::Images;
            continue;
        }
        if lower.starts_with("containers space usage") {
            section = Section::Containers;
            continue;
        }
        if lower.starts_with("local volumes space usage") {
            section = Section::Volumes;
            continue;
        }
        if lower.starts_with("build cache usage") {
            // Header line itself carries the total: "Build cache usage: 1.2GB"
            if let Some(pos) = trimmed.rfind(':') {
                overview.build_cache_bytes = parse_human_size(&trimmed[pos + 1..]);
            }
            section = Section::BuildCache;
            continue;
        }
        if trimmed.starts_with("REPOSITORY")
            || trimmed.starts_with("CONTAINER ID")
            || trimmed.starts_with("VOLUME NAME")
            || trimmed.starts_with("CACHE ID")
        {
            continue;
        }
        let cols = split_columns(trimmed);
        match section {
            Section::Images => {
                // REPOSITORY TAG IMAGE ID CREATED SIZE SHARED SIZE UNIQUE SIZE CONTAINERS
                if cols.len() >= 7 {
                    let size = parse_human_size(&cols[4]);
                    let shared = parse_human_size(&cols[5]);
                    let unique = parse_human_size(&cols[6]);
                    let containers: i64 = cols.get(7).and_then(|v| v.parse().ok()).unwrap_or(0);
                    overview.images_total_bytes += unique + shared;
                    if containers == 0 {
                        overview.images_reclaimable_bytes += unique;
                    }
                    overview.images.push(DockerImage {
                        repository: cols[0].clone(),
                        tag: cols[1].clone(),
                        id: cols[2].clone(),
                        size_bytes: size,
                        shared_bytes: shared,
                        unique_bytes: unique,
                        containers,
                    });
                }
            }
            Section::Containers => {
                // CONTAINER ID IMAGE COMMAND LOCAL VOLUMES SIZE CREATED STATUS NAMES
                if cols.len() >= 7 {
                    let size = parse_size_with_percent(&cols[4]);
                    let name = cols.last().cloned().unwrap_or_default();
                    let status = if cols.len() >= 7 {
                        cols[cols.len() - 2].clone()
                    } else {
                        String::new()
                    };
                    let state = if status.to_lowercase().starts_with("up") {
                        "running"
                    } else if status.to_lowercase().starts_with("exited")
                        || status.to_lowercase().starts_with("created")
                    {
                        "stopped"
                    } else {
                        "unknown"
                    };
                    overview.containers_total_bytes += size;
                    if state == "stopped" {
                        overview.containers_reclaimable_bytes += size;
                    }
                    overview.containers.push(DockerContainer {
                        id: cols[0].clone(),
                        image: cols[1].clone(),
                        name,
                        state: state.to_string(),
                        size_bytes: size,
                        compose_project: None,
                    });
                }
            }
            Section::Volumes => {
                // VOLUME NAME LINKS SIZE
                if cols.len() >= 3 {
                    let size = parse_human_size(&cols[2]);
                    let links: i64 = cols[1].parse().unwrap_or(0);
                    overview.volumes_total_bytes += size;
                    if links == 0 {
                        // Listed for awareness only: volumes are never removed.
                        overview.volumes_reclaimable_bytes += 0;
                    }
                    overview.volumes.push(DockerVolume {
                        name: cols[0].clone(),
                        links,
                        size_bytes: size,
                    });
                }
            }
            Section::BuildCache => {
                // CACHE ID TYPE SIZE CREATED LAST USED USAGE SHARED
                if cols.len() >= 3 && overview.build_cache_bytes == 0 {
                    overview.build_cache_bytes += parse_human_size(&cols[2]);
                } else if cols.len() >= 3 {
                    // Additional rows are already included in the header
                    // total when present; only accumulate without a header.
                }
            }
            Section::None => {}
        }
    }
    overview
}

pub const SAMPLE_DF_V: &str = r#"Images space usage:

REPOSITORY   TAG        IMAGE ID       CREATED        SIZE      SHARED SIZE   UNIQUE SIZE   CONTAINERS
myapp        latest     abc123def456   2 days ago     250MB     100MB         150MB         1
oldimage     v1         def456abc123   3 weeks ago    80MB      100MB         0B            0

Containers space usage:

CONTAINER ID   IMAGE     COMMAND                  LOCAL VOLUMES   SIZE      CREATED       STATUS                   NAMES
aaa111         myapp     "run server"             0               12.5MB    2 days ago    Up 2 days                web
bbb222         oldimage  "run worker"             1               3MB       3 weeks ago   Exited (0) 3 weeks ago   worker

Local Volumes space usage:

VOLUME NAME   LINKS   SIZE
appdata       1       45MB
orphanvol     0       20MB

Build cache usage: 1.5GB

CACHE ID       TYPE        SIZE      CREATED       LAST USED     USAGE     SHARED
cache1         regular     900MB     2 days ago    2 days ago    1         true
cache2         regular     600MB     3 days ago    3 days ago    0         false
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_human_size("1.5GB"), 1_500_000_000);
        assert_eq!(parse_human_size("250MB"), 250_000_000);
        assert_eq!(parse_human_size("0B"), 0);
        assert_eq!(parse_human_size("12.5MB"), 12_500_000);
    }

    #[test]
    fn parses_sample() {
        let o = parse_system_df_v(SAMPLE_DF_V);
        assert_eq!(o.images.len(), 2);
        assert_eq!(o.containers.len(), 2);
        assert_eq!(o.volumes.len(), 2);
        assert_eq!(o.build_cache_bytes, 1_500_000_000);
        assert_eq!(o.containers[0].state, "running");
        assert_eq!(o.containers[1].state, "stopped");
    }
}
