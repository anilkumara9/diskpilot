//! Software updates: the `winget` runs behind
//! `list_available_updates` and `update_software`.
//!
//! The webview names package IDs; this module trusts none of them.
//! Every update run first re-lists what winget itself reports, and
//! an ID is acted on only if it is both well-formed
//! (`diskpilot_core::winget::is_valid_package_id`) and a member of
//! that fresh list. Winget is always invoked as an argv vector —
//! never through a shell — with its output captured on reader
//! threads (winget is chatty: progress bars would deadlock a full
//! pipe) and a hard timeout after which the process is killed.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use diskpilot_core::model::now_ms;
use diskpilot_core::winget::{is_valid_package_id, parse_winget_upgrade, AvailableUpdate};

use crate::commands::with_history;
use crate::ShellState;

type CmdResult<T> = Result<T, String>;

const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(600);

/// The update list as the frontend consumes it: `available` is
/// false when winget itself is not on this machine (the UI shows
/// its not-available state); `error` is reserved for "winget ran
/// but failed", with a short excerpt of what it said.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WingetListResult {
    pub available: bool,
    pub updates: Vec<AvailableUpdate>,
    pub error: Option<String>,
}

/// One package's update outcome. `success` is winget's exit code
/// and nothing else; `message` is the last line winget printed (or
/// the reason the package was refused before anything ran).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    pub id: String,
    pub success: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Process runner
// ---------------------------------------------------------------------------

struct ProcOutput {
    /// Process exit code (`None` when the OS reported none).
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

enum RunFailure {
    /// The process could not be started at all.
    Spawn(String),
    /// Started but had to be killed at the deadline.
    Timeout(Duration),
    /// Waiting on the process failed.
    Wait(String),
}

/// Run a program with captured output and a hard timeout. Reader
/// threads drain both pipes while the caller polls, so a verbose
/// child can never block on a full pipe.
fn run_capture(program: &PathBuf, args: &[&str], timeout: Duration) -> Result<ProcOutput, RunFailure> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A console window flashing over the UI for every winget call
    // would be its own bug; suppress it where the platform has one.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| RunFailure::Spawn(format!("could not start {}: {e}", program.display())))?;
    let mut out_pipe = child.stdout.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = out_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let mut err_pipe = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = err_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunFailure::Timeout(timeout));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(RunFailure::Wait(format!("could not wait for winget: {e}"))),
        }
    };
    let stdout = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    Ok(ProcOutput {
        exit_code: status.code(),
        stdout,
        stderr,
    })
}

// ---------------------------------------------------------------------------
// Winget discovery + listing
// ---------------------------------------------------------------------------

/// Locate `winget.exe`: the PATH first, then the per-user
/// WindowsApps alias location where the App Installer package puts
/// it (that folder is on the PATH for interactive users, but a
/// trimmed environment may not carry it).
fn find_winget() -> Option<PathBuf> {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("winget.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let candidate = PathBuf::from(local).join(r"Microsoft\WindowsApps\winget.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `winget upgrade` once and parse what it reported.
fn fetch_updates(winget: &PathBuf) -> Result<Vec<AvailableUpdate>, String> {
    match run_capture(
        winget,
        &["upgrade", "--include-unknown", "--accept-source-agreements"],
        LIST_TIMEOUT,
    ) {
        Ok(out) => {
            let updates = parse_winget_upgrade(&out.stdout);
            if out.exit_code == Some(0) || !updates.is_empty() {
                return Ok(updates);
            }
            // Winget exits non-zero on the no-upgrades path in some
            // builds; its own wording is the tell, and an empty list
            // is the honest answer there. Anything else is a failure
            // worth showing.
            let combined = format!("{}\n{}", out.stdout, out.stderr);
            if combined.contains("No available upgrade")
                || combined.contains("No newer version")
                || combined.contains("No installed package found")
            {
                return Ok(Vec::new());
            }
            Err(excerpt(&combined, 300))
        }
        Err(RunFailure::Spawn(message)) => Err(message),
        Err(RunFailure::Timeout(limit)) => Err(format!(
            "winget did not answer within {}s",
            limit.as_secs()
        )),
        Err(RunFailure::Wait(message)) => Err(message),
    }
}

/// The last `max_chars` of `text`, trimmed — the tail of a failed
/// run is where the explanation lives.
fn excerpt(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    let start = chars.len().saturating_sub(max_chars);
    chars[start..].iter().collect()
}

/// The last non-empty line of a run's combined output, capped —
/// winget ends an update with its verdict ("Successfully
/// installed" / the failure), which is the line worth showing.
fn last_line_message(out: &ProcOutput) -> String {
    let combined = format!("{}\n{}", out.stdout, out.stderr);
    let line = combined
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        match out.exit_code {
            Some(code) => format!("winget exited with code {code}"),
            None => "winget exited without a status code".to_string(),
        }
    } else {
        excerpt(line, 200)
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Packages with a newer version available, per winget. Winget
/// missing from the machine is a state, not an error.
#[tauri::command]
pub async fn list_available_updates() -> CmdResult<WingetListResult> {
    tauri::async_runtime::spawn_blocking(|| match find_winget() {
        None => WingetListResult {
            available: false,
            updates: Vec::new(),
            error: None,
        },
        Some(winget) => match fetch_updates(&winget) {
            Ok(updates) => WingetListResult {
                available: true,
                updates,
                error: None,
            },
            Err(message) => WingetListResult {
                available: true,
                updates: Vec::new(),
                error: Some(message),
            },
        },
    })
    .await
    .map_err(|e| format!("background task failed: {e}"))
}

/// Update the named packages — or, with an empty `ids`, every
/// package the fresh list reports. Each requested ID is validated
/// against that fresh list before anything runs: an ID winget did
/// not just report (or one that is not even well-formed) is refused
/// in place, without invoking winget for it. The run is recorded to
/// History.
#[tauri::command]
pub async fn update_software(
    state: State<'_, ShellState>,
    ids: Vec<String>,
) -> CmdResult<Vec<UpdateOutcome>> {
    let outcomes = tauri::async_runtime::spawn_blocking(move || -> Vec<UpdateOutcome> {
        let Some(winget) = find_winget() else {
            return ids
                .iter()
                .map(|id| UpdateOutcome {
                    id: id.clone(),
                    success: false,
                    message: "winget is not available on this PC".to_string(),
                })
                .collect();
        };
        let fresh = match fetch_updates(&winget) {
            Ok(updates) => updates,
            Err(message) => {
                return ids
                    .iter()
                    .map(|id| UpdateOutcome {
                        id: id.clone(),
                        success: false,
                        message: message.clone(),
                    })
                    .collect();
            }
        };
        // Empty request = update everything the fresh list reports.
        let requested: Vec<String> = if ids.is_empty() {
            fresh.iter().map(|u| u.id.clone()).collect()
        } else {
            ids
        };
        let mut seen: Vec<String> = Vec::new();
        let mut results: Vec<UpdateOutcome> = Vec::new();
        for id in &requested {
            if seen.contains(id) {
                continue; // a duplicate request updates (and reports) once
            }
            seen.push(id.clone());
            if !is_valid_package_id(id) || !fresh.iter().any(|u| &u.id == id) {
                results.push(UpdateOutcome {
                    id: id.clone(),
                    success: false,
                    message: "Not in the current update list".to_string(),
                });
                continue;
            }
            let outcome = match run_capture(
                &winget,
                &[
                    "upgrade",
                    "--id",
                    id,
                    "--exact",
                    "--silent",
                    "--disable-interactivity",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ],
                UPDATE_TIMEOUT,
            ) {
                Ok(out) => UpdateOutcome {
                    id: id.clone(),
                    success: out.exit_code == Some(0),
                    message: last_line_message(&out),
                },
                Err(RunFailure::Timeout(limit)) => UpdateOutcome {
                    id: id.clone(),
                    success: false,
                    message: format!("update timed out after {}s", limit.as_secs()),
                },
                Err(RunFailure::Spawn(message)) | Err(RunFailure::Wait(message)) => {
                    UpdateOutcome {
                        id: id.clone(),
                        success: false,
                        message,
                    }
                }
            };
            results.push(outcome);
        }
        results
    })
    .await
    .map_err(|e| format!("background task failed: {e}"))?;
    let succeeded: Vec<String> = outcomes
        .iter()
        .filter(|o| o.success)
        .map(|o| o.id.clone())
        .collect();
    let errors: Vec<String> = outcomes
        .iter()
        .filter(|o| !o.success)
        .map(|o| format!("{}: {}", o.id, o.message))
        .collect();
    let detail = format!(
        "Updated {} of {} packages",
        succeeded.len(),
        outcomes.len()
    );
    let _ = with_history(&state, |history| {
        history
            .record_action(
                now_ms(),
                "software-update",
                &succeeded,
                0,
                &errors,
                Some(&detail),
                diskpilot_core::history::RemovedItemMode::Permanent,
            )
            .map(|_| ())
    });
    Ok(outcomes)
}
