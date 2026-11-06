//! Read-only scan CLI: `diskpilot-scan [path]` prints the ScanSummary as
//! JSON to stdout. Defaults to the user profile folder.
//!
//! Exit codes: 0 on success, 2 for an invalid scan root (missing or not a
//! directory), 1 when the summary cannot be serialised.

use diskpilot_core::scanner::Scanner;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    let root = match arg {
        Some(p) => PathBuf::from(p),
        None => dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
    };
    if !root.exists() {
        eprintln!("scan root does not exist: {}", root.display());
        return ExitCode::from(2);
    }
    if !root.is_dir() {
        eprintln!("scan root is not a directory: {}", root.display());
        return ExitCode::from(2);
    }
    let scanner = Scanner::new(&root);
    let (summary, _index) = scanner.scan();
    match serde_json::to_string_pretty(&summary) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("failed to serialise summary: {}", e);
            ExitCode::FAILURE
        }
    }
}
