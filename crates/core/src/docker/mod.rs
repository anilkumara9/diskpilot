pub mod actions;
pub mod cli;
pub mod parse;

pub use actions::DockerAction;
pub use parse::parse_system_df_v;

use crate::model::DockerOverview;
use std::path::Path;

/// Docker Desktop virtual disk location(s) on Windows; also returns the
/// conventional path on other systems for reporting.
pub fn vhdx_candidates(home: &Path) -> Vec<std::path::PathBuf> {
    let local = dirs::data_local_dir().unwrap_or_else(|| home.join("AppData").join("Local"));
    vec![
        local
            .join("Docker")
            .join("wsl")
            .join("data")
            .join("ext4.vhdx"),
        local
            .join("Docker")
            .join("wsl")
            .join("distro")
            .join("ext4.vhdx"),
        local.join("Docker").join("wsl").join("ext4.vhdx"),
    ]
}

pub fn vhdx_size(home: &Path) -> Option<(std::path::PathBuf, u64)> {
    for cand in vhdx_candidates(home) {
        if let Ok(md) = std::fs::metadata(&cand) {
            return Some((cand, md.len()));
        }
    }
    None
}

/// Full overview: states are not installed / not running / ready.
pub fn get_overview() -> DockerOverview {
    let Some(docker) = cli::locate_docker() else {
        return DockerOverview {
            installed: false,
            running: false,
            detail: "Docker CLI was not found in known locations or on PATH.".to_string(),
            ..Default::default()
        };
    };
    match cli::run_with_timeout(&docker, &["system", "df", "-v"], cli::DOCKER_TIMEOUT) {
        Ok(out) if out.success => {
            let mut overview = parse::parse_system_df_v(&out.stdout);
            overview.installed = true;
            overview.running = true;
            // Link containers to images by name for display purposes.
            for c in &mut overview.containers {
                if overview
                    .images
                    .iter()
                    .any(|i| format!("{}:{}", i.repository, i.tag) == c.image || i.id == c.image)
                {
                    // already linked by reference; nothing else needed
                }
            }
            overview
        }
        Ok(out) => DockerOverview {
            installed: true,
            running: false,
            detail: format!(
                "Docker is installed but the engine did not respond: {}",
                out.stderr.trim()
            ),
            ..Default::default()
        },
        Err(e) => DockerOverview {
            installed: true,
            running: false,
            detail: format!("Docker is installed but could not be queried: {}", e),
            ..Default::default()
        },
    }
}

/// The outcome of an executed docker action: the command line as
/// shown to the user beforehand, what the process printed, and
/// whether it exited successfully. A process that RAN and failed
/// is an `Ok` result with `success: false` — its output is the
/// explanation the user needs — while `Err` is reserved for the
/// action never running at all (no CLI, failed re-check, safety
/// invariant, spawn/timeout failure).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionResult {
    /// The preview command line (`docker container rm …`).
    pub command: String,
    /// The process output: stdout on success; on failure, stderr
    /// when the process wrote any, stdout otherwise.
    pub output: String,
    /// The process exit status. Defaults to false so payloads
    /// written before this field existed deserialize as "not known
    /// to have succeeded".
    #[serde(default)]
    pub success: bool,
}

pub fn execute_action(action: &DockerAction) -> Result<DockerActionResult, String> {
    let docker = cli::locate_docker().ok_or_else(|| "Docker CLI not found".to_string())?;
    // Re-check against a fresh overview before running anything.
    let overview = get_overview();
    action.recheck(&overview).map_err(|e| e.to_string())?;
    if !actions::assert_action_args_safe(action) {
        return Err("action failed the safety invariant check".to_string());
    }
    let args = action.args();
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = cli::run_with_timeout(&docker, &refs, cli::DOCKER_TIMEOUT)?;
    let output = if out.success {
        out.stdout
    } else if !out.stderr.trim().is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    Ok(DockerActionResult {
        command: action.preview_command(),
        output,
        success: out.success,
    })
}
