//! Transcript credential scan. Offline pattern matching for well-known
//! credential shapes; output is masked only — the full value never leaves
//! this module, is never stored, and nothing is modified.

use crate::model::SecretFinding;
use std::path::Path;

struct Pattern {
    kind: &'static str,
    prefix: &'static str,
    min_len: usize,
}

fn patterns() -> Vec<Pattern> {
    vec![
        Pattern {
            kind: "Anthropic API key",
            prefix: "sk-ant-",
            min_len: 20,
        },
        Pattern {
            kind: "OpenAI API key",
            prefix: "sk-proj-",
            min_len: 20,
        },
        Pattern {
            kind: "OpenAI API key",
            prefix: "sk-",
            min_len: 20,
        },
        Pattern {
            kind: "AWS access key id",
            prefix: "AKIA",
            min_len: 20,
        },
        Pattern {
            kind: "GitHub personal access token",
            prefix: "ghp_",
            min_len: 20,
        },
        Pattern {
            kind: "GitHub fine-grained token",
            prefix: "github_pat_",
            min_len: 20,
        },
        Pattern {
            kind: "GitHub OAuth token",
            prefix: "gho_",
            min_len: 20,
        },
        Pattern {
            kind: "Slack token",
            prefix: "xoxb-",
            min_len: 15,
        },
        Pattern {
            kind: "Slack token",
            prefix: "xoxp-",
            min_len: 15,
        },
        Pattern {
            kind: "Slack token",
            prefix: "xoxa-",
            min_len: 15,
        },
        Pattern {
            kind: "Slack token",
            prefix: "xoxr-",
            min_len: 15,
        },
        Pattern {
            kind: "Slack token",
            prefix: "xoxs-",
            min_len: 15,
        },
        Pattern {
            kind: "Google API key",
            prefix: "AIza",
            min_len: 30,
        },
        Pattern {
            kind: "Stripe secret key",
            prefix: "sk_live_",
            min_len: 20,
        },
        Pattern {
            kind: "Stripe restricted key",
            prefix: "rk_live_",
            min_len: 20,
        },
    ]
}

const PRIVATE_KEY_HEADERS: &[&str] = &[
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "-----BEGIN EC PRIVATE KEY-----",
    "-----BEGIN PRIVATE KEY-----",
];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+' | '=')
}

pub fn mask(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "********".to_string();
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().skip(chars.len() - 4).collect();
    format!("{}…{} ({} chars)", head, tail, chars.len())
}

pub fn scan_text(path: &str, text: &str) -> Vec<SecretFinding> {
    let mut out: Vec<SecretFinding> = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        for header in PRIVATE_KEY_HEADERS {
            if line.contains(header) {
                out.push(SecretFinding {
                    path: path.to_string(),
                    kind: "Private key".to_string(),
                    masked: mask(header),
                    line: idx + 1,
                });
            }
        }
        for pat in patterns() {
            let mut search_from = 0usize;
            while let Some(pos) = line[search_from..].find(pat.prefix) {
                let start = search_from + pos;
                let rest = &line[start..];
                let token: String = rest.chars().take_while(|c| is_token_char(*c)).collect();
                // Avoid double-reporting sk-ant-/sk-proj- under the bare sk- pattern.
                if pat.prefix == "sk-"
                    && (token.starts_with("sk-ant-") || token.starts_with("sk-proj-"))
                {
                    search_from = start + token.len().max(1);
                    continue;
                }
                if token.len() >= pat.min_len {
                    out.push(SecretFinding {
                        path: path.to_string(),
                        kind: pat.kind.to_string(),
                        masked: mask(&token),
                        line: idx + 1,
                    });
                }
                search_from = start + token.len().max(1);
                if search_from >= line.len() {
                    break;
                }
            }
        }
    }
    out
}

pub fn scan_file(path: &Path) -> Vec<SecretFinding> {
    match std::fs::read_to_string(path) {
        Ok(text) => scan_text(&path.to_string_lossy(), &text),
        Err(_) => Vec::new(),
    }
}

/// Scan transcript-like files (jsonl / txt / md / log) under a directory.
pub fn scan_directory(root: &Path) -> Vec<SecretFinding> {
    let mut out = Vec::new();
    scan_dir_inner(root, &mut out, 0);
    out
}

fn scan_dir_inner(dir: &Path, out: &mut Vec<SecretFinding>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if md.file_type().is_symlink() {
            continue;
        }
        if md.is_dir() {
            scan_dir_inner(&path, out, depth + 1);
        } else if md.is_file() {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if matches!(ext.as_str(), "jsonl" | "txt" | "md" | "log" | "json")
                && md.len() < 20_000_000
            {
                out.extend(scan_file(&path));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_finds() {
        let text = "key is sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789 ok";
        let found = scan_text("t.jsonl", text);
        assert_eq!(found.len(), 1);
        assert!(!found[0].masked.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(found[0].masked.contains("sk-a"));
    }

    #[test]
    fn ignores_short_tokens() {
        assert!(scan_text("t", "sk-short").is_empty());
    }
}
