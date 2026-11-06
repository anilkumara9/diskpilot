//! Closed set of Docker actions. Every action previews as an exact command,
//! is re-checked before it runs, and follows hard rules: no force flags on
//! removals, volumes are never removed, builder prune only for build cache.

use crate::error::{CoreError, CoreResult};
use crate::model::DockerOverview;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerAction {
    RemoveStoppedContainer { id: String, name: String },
    RemoveUnusedImage { id: String, reference: String },
    PruneBuildCache,
}

impl DockerAction {
    /// The exact argument vector (after the docker binary) for this action.
    pub fn args(&self) -> Vec<String> {
        match self {
            DockerAction::RemoveStoppedContainer { id, .. } => {
                vec!["container".into(), "rm".into(), id.clone()]
            }
            DockerAction::RemoveUnusedImage { id, .. } => {
                vec!["image".into(), "rm".into(), id.clone()]
            }
            DockerAction::PruneBuildCache => vec![
                "builder".into(),
                "prune".into(),
                "--all".into(),
                "--force".into(),
            ],
        }
    }

    pub fn preview_command(&self) -> String {
        format!("docker {}", self.args().join(" "))
    }

    pub fn describe(&self) -> String {
        match self {
            DockerAction::RemoveStoppedContainer { name, .. } => {
                format!("Remove stopped container {}", name)
            }
            DockerAction::RemoveUnusedImage { reference, .. } => {
                format!("Remove unused image {}", reference)
            }
            DockerAction::PruneBuildCache => "Prune unused build cache".to_string(),
        }
    }

    /// Validate the action against a fresh overview before running it.
    pub fn recheck(&self, overview: &DockerOverview) -> CoreResult<()> {
        match self {
            DockerAction::RemoveStoppedContainer { id, name } => {
                let c = overview
                    .containers
                    .iter()
                    .find(|c| &c.id == id || &c.name == name)
                    .ok_or_else(|| {
                        CoreError::DockerUnavailable(format!("container {} no longer exists", name))
                    })?;
                if c.state == "running" {
                    return Err(CoreError::SafetyRejected(format!(
                        "container {} is running; only stopped containers can be removed",
                        name
                    )));
                }
                Ok(())
            }
            DockerAction::RemoveUnusedImage { id, reference } => {
                let img = overview
                    .images
                    .iter()
                    .find(|i| &i.id == id || format!("{}:{}", i.repository, i.tag) == *reference)
                    .ok_or_else(|| {
                        CoreError::DockerUnavailable(format!(
                            "image {} no longer exists",
                            reference
                        ))
                    })?;
                if img.containers > 0 {
                    return Err(CoreError::SafetyRejected(format!(
                        "image {} is used by {} container(s)",
                        reference, img.containers
                    )));
                }
                Ok(())
            }
            DockerAction::PruneBuildCache => {
                if overview.build_cache_bytes == 0 {
                    return Err(CoreError::DockerUnavailable(
                        "no build cache to prune".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Invariants over every action's argument vector.
pub fn assert_action_args_safe(action: &DockerAction) -> bool {
    let args = action.args();
    match action {
        DockerAction::RemoveStoppedContainer { .. } | DockerAction::RemoveUnusedImage { .. } => {
            !args
                .iter()
                .any(|a| a == "-f" || a == "--force" || a == "-v" || a == "--volumes")
        }
        DockerAction::PruneBuildCache => args.first().map(|a| a.as_str()) == Some("builder"),
    }
}

/// Steps for compacting the Docker Desktop virtual disk on Windows. These are
/// shown to the user, never executed by the app.
pub fn wsl_compaction_steps(vhdx_path: &str) -> Vec<String> {
    vec![
        "Quit Docker Desktop completely.".to_string(),
        "Run: wsl --shutdown".to_string(),
        format!(
            "Compact the virtual disk at {} using Optimize-VHD (Hyper-V) or diskpart: select vdisk file=\"{}\" then compact vdisk.",
            vhdx_path, vhdx_path
        ),
        "Start Docker Desktop again and confirm the reported disk size.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_actions_have_no_force_or_volume_flags() {
        let a = DockerAction::RemoveStoppedContainer {
            id: "abc".into(),
            name: "web".into(),
        };
        assert!(assert_action_args_safe(&a));
        assert_eq!(a.preview_command(), "docker container rm abc");
        let b = DockerAction::PruneBuildCache;
        assert_eq!(b.preview_command(), "docker builder prune --all --force");
        assert!(assert_action_args_safe(&b));
    }
}
