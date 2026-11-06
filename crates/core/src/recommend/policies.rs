//! Recommendation policies as data: classification -> tier, methods, and the
//! explanation text shown to the user. Recommendations explain only; they
//! never delete anything themselves.

use crate::model::{CleanupMethod, Confidence, Tier};

#[derive(Debug, Clone)]
pub struct Policy {
    pub tier: Tier,
    pub methods: Vec<CleanupMethod>,
    pub why_safe: &'static str,
    pub risks: &'static str,
    pub confidence: Confidence,
    pub alternative_command: Option<&'static str>,
    pub regenerable: bool,
    pub title_prefix: &'static str,
}

pub fn policy_for_rule(rule_id: &str) -> Option<Policy> {
    let p = match rule_id {
        "cache.temp" | "cache.generic" | "cache.npm" | "cache.yarn" | "cache.nuget"
        | "cache.pip" | "cache.cargo-registry" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Cache contents are downloaded or generated again on demand by the tool that owns them.",
            risks: "The next run of the owning tool may be slower while it rebuilds the cache.",
            confidence: Confidence::High,
            alternative_command: None,
            regenerable: true,
            title_prefix: "Cache",
        },
        "cache.pnpm-store" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::RunCommand],
            why_safe: "pnpm can prune store entries that no project references any more.",
            risks: "Removing the store by hand can break hard-linked project installs; use the pnpm command instead.",
            confidence: Confidence::High,
            alternative_command: Some("pnpm store prune"),
            regenerable: true,
            title_prefix: "pnpm store",
        },
        "cache.gradle" => Policy {
            tier: Tier::ReviewFirst,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Gradle re-downloads dependencies and rebuilds its caches on the next build.",
            risks: "The first build after cleaning needs network access and takes longer.",
            confidence: Confidence::Medium,
            alternative_command: None,
            regenerable: true,
            title_prefix: "Gradle cache",
        },
        "logs.folder" | "ext.log" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Logs are diagnostic output; applications keep writing new ones.",
            risks: "Recent logs can help diagnose a current problem; check dates if you are mid-investigation.",
            confidence: Confidence::High,
            alternative_command: None,
            regenerable: true,
            title_prefix: "Logs",
        },
        "build.node-modules" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Dependencies are restored exactly from the project lockfile with the package manager install command.",
            risks: "The project will not run until dependencies are installed again.",
            confidence: Confidence::High,
            alternative_command: Some("npm install (or pnpm / yarn install) in the project folder"),
            regenerable: true,
            title_prefix: "Node dependencies",
        },
        "build.rust-target" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Build output is fully regenerated from source on the next build.",
            risks: "The next build is a full rebuild and can take a while on large projects.",
            confidence: Confidence::High,
            alternative_command: Some("cargo clean (in the project folder)"),
            regenerable: true,
            title_prefix: "Rust build output",
        },
        "build.output" | "build.dotnet" => Policy {
            tier: Tier::LikelySafe,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "Build output is regenerated from source on the next build.",
            risks: "The next build is a full rebuild.",
            confidence: Confidence::Medium,
            alternative_command: None,
            regenerable: true,
            title_prefix: "Build output",
        },
        "dev.venv" => Policy {
            tier: Tier::ReviewFirst,
            methods: vec![CleanupMethod::MoveToTrash],
            why_safe: "The environment can be recreated from the project requirements file.",
            risks: "If the project has no pinned requirements, recreating the exact environment may be difficult.",
            confidence: Confidence::Medium,
            alternative_command: Some("python -m venv .venv && .venv\\Scripts\\pip install -r requirements.txt"),
            regenerable: true,
            title_prefix: "Python virtual environment",
        },
        "docker.wsl" | "docker.data" => Policy {
            tier: Tier::ReviewFirst,
            methods: vec![CleanupMethod::ManualReview],
            why_safe: "Space is reclaimed safely through Docker's own commands, which know which layers are still referenced.",
            risks: "Deleting Docker files directly can corrupt images and volumes, including data you still need.",
            confidence: Confidence::High,
            alternative_command: Some("Use the Docker section to preview and run Docker's own cleanup commands"),
            regenerable: false,
            title_prefix: "Docker storage",
        },
        "dev.git" => Policy {
            tier: Tier::Keep,
            methods: vec![CleanupMethod::ManualReview],
            why_safe: "Repository history is usually the only copy of past work.",
            risks: "Removing .git destroys history, branches and unpushed commits.",
            confidence: Confidence::High,
            alternative_command: Some("git gc (in the repository) compacts history without removing it"),
            regenerable: false,
            title_prefix: "Git data",
        },
        _ => return None,
    };
    Some(p)
}

pub fn session_policy() -> Policy {
    Policy {
        tier: Tier::ReviewFirst,
        methods: vec![CleanupMethod::MoveToTrash],
        why_safe: "These sessions belong to projects that no longer exist on disk, so nothing current references them.",
        risks: "Session transcripts cannot be recreated; they may hold context or decisions you want to keep.",
        confidence: Confidence::Medium,
        alternative_command: None,
        regenerable: false,
        title_prefix: "Sessions for a removed project",
    }
}

pub fn keep_policy() -> Policy {
    Policy {
        tier: Tier::Keep,
        methods: vec![CleanupMethod::ManualReview],
        why_safe: "Listed for awareness only; this is personal data with no automatic cleanup.",
        risks: "Deleting personal files is permanent once the Recycle Bin is emptied.",
        confidence: Confidence::High,
        alternative_command: None,
        regenerable: false,
        title_prefix: "Personal data",
    }
}
