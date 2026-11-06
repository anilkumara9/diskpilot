//! Read-only scan CLI: `diskpilot-scan [path]` prints the ScanSummary as
//! JSON to stdout. Defaults to the user profile folder.

use diskpilot_core::scanner::Scanner;
use std::path::PathBuf;

fn main() {
    let arg = std::env::args().nth(1);
    let root = match arg {
        Some(p) => PathBuf::from(p),
        None => dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
    };
    if !root.exists() {
        eprintln!("scan root does not exist: {}", root.display());
        std::process::exit(2);
    }
    let scanner = Scanner::new(&root);
    let (summary, _index) = scanner.scan();
    match serde_json::to_string_pretty(&summary) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("failed to serialise summary: {}", e);
            std::process::exit(1);
        }
    }
}
