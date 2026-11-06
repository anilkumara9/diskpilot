//! Data table of local AI agent storage locations and how each kind of data
//! maps to a cleanup tier. Analyzers walk this table; nothing here touches
//! the filesystem.

use crate::model::Tier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDataKind {
    Versions,
    Sessions,
    Logs,
    Cache,
    Updates,
    Checkpoints,
    History,
    Settings,
    ShellSnapshots,
    DebugLogs,
    McpLogs,
    Data,
}

impl AgentDataKind {
    pub fn tier(&self) -> Tier {
        match self {
            AgentDataKind::Cache
            | AgentDataKind::Logs
            | AgentDataKind::Updates
            | AgentDataKind::DebugLogs
            | AgentDataKind::McpLogs => Tier::LikelySafe,
            AgentDataKind::Checkpoints
            | AgentDataKind::Sessions
            | AgentDataKind::ShellSnapshots => Tier::ReviewFirst,
            AgentDataKind::History
            | AgentDataKind::Settings
            | AgentDataKind::Versions
            | AgentDataKind::Data => Tier::Keep,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            AgentDataKind::Versions => "Versions",
            AgentDataKind::Sessions => "Sessions",
            AgentDataKind::Logs => "Logs",
            AgentDataKind::Cache => "Cache",
            AgentDataKind::Updates => "Update downloads",
            AgentDataKind::Checkpoints => "Edit checkpoints",
            AgentDataKind::History => "History",
            AgentDataKind::Settings => "Settings",
            AgentDataKind::ShellSnapshots => "Shell snapshots",
            AgentDataKind::DebugLogs => "Debug logs",
            AgentDataKind::McpLogs => "MCP logs",
            AgentDataKind::Data => "Data",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AgentEntrySpec {
    pub rel: &'static str,
    pub kind: AgentDataKind,
    pub detail: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentSpec {
    pub tool: &'static str,
    /// Home folder relative to the user profile ("" entries use roaming data).
    pub home_rel: &'static str,
    pub entries: &'static [AgentEntrySpec],
}

const CLAUDE_ENTRIES: &[AgentEntrySpec] = &[
    AgentEntrySpec {
        rel: "projects",
        kind: AgentDataKind::Sessions,
        detail: "Session transcripts per project, including projects that no longer exist.",
    },
    AgentEntrySpec {
        rel: "todos",
        kind: AgentDataKind::Data,
        detail: "Task lists from sessions.",
    },
    AgentEntrySpec {
        rel: "shell-snapshots",
        kind: AgentDataKind::ShellSnapshots,
        detail: "Shell environment snapshots taken at session start.",
    },
    AgentEntrySpec {
        rel: "debug",
        kind: AgentDataKind::DebugLogs,
        detail: "Debug logs from the CLI.",
    },
    AgentEntrySpec {
        rel: "logs",
        kind: AgentDataKind::Logs,
        detail: "Log output.",
    },
    AgentEntrySpec {
        rel: "cache",
        kind: AgentDataKind::Cache,
        detail: "Download and response caches.",
    },
    AgentEntrySpec {
        rel: "file-history",
        kind: AgentDataKind::Checkpoints,
        detail: "Edit checkpoints used to undo file changes.",
    },
    AgentEntrySpec {
        rel: "history.jsonl",
        kind: AgentDataKind::History,
        detail: "Prompt history; reported only, never recommended.",
    },
    AgentEntrySpec {
        rel: "settings.json",
        kind: AgentDataKind::Settings,
        detail: "Settings; reported only.",
    },
    AgentEntrySpec {
        rel: "statsig",
        kind: AgentDataKind::Cache,
        detail: "Feature-flag cache.",
    },
];

const CODEX_ENTRIES: &[AgentEntrySpec] = &[
    AgentEntrySpec {
        rel: "sessions",
        kind: AgentDataKind::Sessions,
        detail: "Session transcripts; listed for awareness, never recommended for removal.",
    },
    AgentEntrySpec {
        rel: "log",
        kind: AgentDataKind::Logs,
        detail: "CLI and app logs.",
    },
    AgentEntrySpec {
        rel: "logs",
        kind: AgentDataKind::Logs,
        detail: "CLI and app logs.",
    },
    AgentEntrySpec {
        rel: "cache",
        kind: AgentDataKind::Cache,
        detail: "Response and download caches.",
    },
    AgentEntrySpec {
        rel: "tmp",
        kind: AgentDataKind::Cache,
        detail: "Temporary working files and update staging.",
    },
    AgentEntrySpec {
        rel: "history.jsonl",
        kind: AgentDataKind::History,
        detail: "Prompt history; reported only, never recommended.",
    },
    AgentEntrySpec {
        rel: "config.toml",
        kind: AgentDataKind::Settings,
        detail: "Configuration; reported only.",
    },
];

const GEMINI_ENTRIES: &[AgentEntrySpec] = &[
    AgentEntrySpec {
        rel: "tmp",
        kind: AgentDataKind::Cache,
        detail: "Temporary files and per-project session scratch data.",
    },
    AgentEntrySpec {
        rel: "sessions",
        kind: AgentDataKind::Sessions,
        detail: "Session data per project.",
    },
    AgentEntrySpec {
        rel: "logs",
        kind: AgentDataKind::Logs,
        detail: "CLI logs.",
    },
    AgentEntrySpec {
        rel: "history",
        kind: AgentDataKind::History,
        detail: "Prompt history; reported only.",
    },
    AgentEntrySpec {
        rel: "settings.json",
        kind: AgentDataKind::Settings,
        detail: "Settings; reported only.",
    },
];

const COPILOT_ENTRIES: &[AgentEntrySpec] = &[
    AgentEntrySpec {
        rel: "logs",
        kind: AgentDataKind::Logs,
        detail: "Copilot CLI logs.",
    },
    AgentEntrySpec {
        rel: "sessions",
        kind: AgentDataKind::Sessions,
        detail: "Copilot session data.",
    },
    AgentEntrySpec {
        rel: "cache",
        kind: AgentDataKind::Cache,
        detail: "Copilot caches.",
    },
    AgentEntrySpec {
        rel: "config.json",
        kind: AgentDataKind::Settings,
        detail: "Configuration; reported only.",
    },
];

pub fn agent_specs() -> Vec<AgentSpec> {
    vec![
        AgentSpec {
            tool: "Claude Code",
            home_rel: ".claude",
            entries: CLAUDE_ENTRIES,
        },
        AgentSpec {
            tool: "Codex",
            home_rel: ".codex",
            entries: CODEX_ENTRIES,
        },
        AgentSpec {
            tool: "Gemini CLI",
            home_rel: ".gemini",
            entries: GEMINI_ENTRIES,
        },
        AgentSpec {
            tool: "GitHub Copilot",
            home_rel: ".copilot",
            entries: COPILOT_ENTRIES,
        },
    ]
}

pub fn tier_for_kind(kind: AgentDataKind) -> Tier {
    kind.tier()
}
