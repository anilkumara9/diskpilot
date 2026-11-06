//! Docker CLI location and execution. GUI apps do not inherit a shell PATH,
//! so the CLI is searched in known install locations first, then PATH.
//! Commands are always argument vectors (never a shell string) with a timeout.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn candidate_locations() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(
            PathBuf::from(pf)
                .join("Docker")
                .join("Docker")
                .join("resources")
                .join("bin")
                .join("docker.exe"),
        );
    }
    if let Some(pf) = std::env::var_os("ProgramW6432") {
        out.push(
            PathBuf::from(pf)
                .join("Docker")
                .join("Docker")
                .join("resources")
                .join("bin")
                .join("docker.exe"),
        );
    }
    if let Some(local) = dirs::data_local_dir() {
        out.push(
            local
                .join("Docker")
                .join("resources")
                .join("bin")
                .join("docker.exe"),
        );
    }
    for dir in ["/usr/local/bin", "/usr/bin", "/opt/homebrew/bin"] {
        out.push(PathBuf::from(dir).join("docker"));
    }
    out
}

pub fn locate_docker() -> Option<PathBuf> {
    for cand in candidate_locations() {
        if cand.exists() {
            return Some(cand);
        }
    }
    // PATH lookup without a shell.
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in ["docker", "docker.exe"] {
                let cand = dir.join(name);
                if cand.exists() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run an argument vector with a timeout, capturing output.
pub fn run_with_timeout(
    program: &PathBuf,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {}: {}", program.display(), e))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let out = child
                    .wait_with_output()
                    .map_err(|e| format!("failed reading output: {}", e))?;
                return Ok(CommandOutput {
                    success: out.status.success(),
                    stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("command timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("failed waiting for command: {}", e)),
        }
    }
}

pub const DOCKER_TIMEOUT: Duration = Duration::from_secs(30);
