//! Classification rule table as plain data. First match wins.
//! Container rules set a category for a subtree but allow deeper matches.
//! Leaf rules claim the whole subtree beneath them.

use crate::model::{Category, Confidence};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    Container,
    Leaf,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: &'static str,
    pub category: Category,
    pub label: &'static str,
    pub explanation: &'static str,
    pub confidence: Confidence,
    pub scope: RuleScope,
    /// Lowercase exact component names this rule matches.
    pub names: &'static [&'static str],
    /// Lowercase path fragments (with separators normalised to `/`).
    pub path_fragments: &'static [&'static str],
}

const C: Confidence = Confidence::High;
const M: Confidence = Confidence::Medium;

pub fn rules() -> Vec<Rule> {
    vec![
        // ---- System (leaf, never cleanable) ----
        Rule { id: "system.recycle-bin", category: Category::SystemFiles, label: "Recycle Bin", explanation: "Windows Recycle Bin storage; only emptied through the dedicated Recycle Bin flow.", confidence: C, scope: RuleScope::Leaf, names: &["$recycle.bin"], path_fragments: &["$recycle.bin"] },
        Rule { id: "system.windows", category: Category::SystemFiles, label: "Windows system files", explanation: "Core operating system files managed by Windows.", confidence: C, scope: RuleScope::Leaf, names: &["winsxs"], path_fragments: &["/windows/", "c:/windows"] },
        Rule { id: "system.program-files", category: Category::SystemFiles, label: "Installed programs", explanation: "Program installation directory managed by installers.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &["program files", "program files (x86)"] },
        Rule { id: "system.swap", category: Category::SystemFiles, label: "System paging files", explanation: "Windows paging and hibernation files managed by the operating system.", confidence: C, scope: RuleScope::Leaf, names: &["hiberfil.sys", "pagefile.sys", "swapfile.sys"], path_fragments: &[] },
        // ---- Docker ----
        Rule { id: "docker.wsl", category: Category::DockerStorage, label: "Docker WSL disk", explanation: "Docker Desktop virtual disk holding images, containers and volumes.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &["/docker/wsl", "localappdata/docker"] },
        Rule { id: "docker.data", category: Category::DockerStorage, label: "Docker data", explanation: "Data owned by the Docker engine; managed through Docker commands, never by deleting files.", confidence: C, scope: RuleScope::Container, names: &[".docker", "docker"], path_fragments: &["/.docker"] },
        // ---- Dependency / build leaves ----
        Rule { id: "build.node-modules", category: Category::BuildArtifacts, label: "Node dependencies", explanation: "Installed Node.js dependencies; regenerable from the project lockfile.", confidence: C, scope: RuleScope::Leaf, names: &["node_modules"], path_fragments: &[] },
        Rule { id: "build.rust-target", category: Category::BuildArtifacts, label: "Rust build output", explanation: "Cargo build output; regenerable with a fresh build.", confidence: C, scope: RuleScope::Leaf, names: &["target"], path_fragments: &[] },
        Rule { id: "build.output", category: Category::BuildArtifacts, label: "Build output", explanation: "Compiled or bundled build output; regenerable from source.", confidence: M, scope: RuleScope::Leaf, names: &["dist", "build", "out", ".next", ".nuxt", ".output"], path_fragments: &[] },
        Rule { id: "build.dotnet", category: Category::BuildArtifacts, label: ".NET build output", explanation: ".NET intermediate or output folder; regenerable with a fresh build.", confidence: M, scope: RuleScope::Leaf, names: &["bin", "obj"], path_fragments: &[] },
        Rule { id: "dev.git", category: Category::DeveloperTools, label: "Git data", explanation: "Git repository history and objects.", confidence: C, scope: RuleScope::Leaf, names: &[".git"], path_fragments: &[] },
        Rule { id: "dev.venv", category: Category::DeveloperTools, label: "Python virtual environment", explanation: "Python virtual environment; recreatable from the project requirements.", confidence: C, scope: RuleScope::Leaf, names: &[".venv", "venv", ".tox"], path_fragments: &[] },
        // ---- Caches ----
        Rule { id: "cache.temp", category: Category::Cache, label: "Temporary files", explanation: "Temporary files created by applications and the operating system.", confidence: C, scope: RuleScope::Leaf, names: &["temp", "tmp", ".tmp"], path_fragments: &["appdata/local/temp", "/temp/"] },
        Rule { id: "cache.npm", category: Category::Cache, label: "npm cache", explanation: "npm download cache; entries are re-downloaded on demand.", confidence: C, scope: RuleScope::Leaf, names: &["npm-cache", "_cacache"], path_fragments: &["npm-cache"] },
        Rule { id: "cache.pnpm-store", category: Category::Cache, label: "pnpm store", explanation: "pnpm content-addressable store shared across projects.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &["pnpm/store", "pnpm-store"] },
        Rule { id: "cache.yarn", category: Category::Cache, label: "Yarn cache", explanation: "Yarn package cache; entries are re-downloaded on demand.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &["yarn/cache", "yarn/berry/cache"] },
        Rule { id: "cache.nuget", category: Category::Cache, label: "NuGet package cache", explanation: "Downloaded NuGet packages; restored on demand by .NET tooling.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &[".nuget/packages", "nuget/cache"] },
        Rule { id: "cache.gradle", category: Category::Cache, label: "Gradle cache", explanation: "Gradle dependency and build cache.", confidence: C, scope: RuleScope::Container, names: &[".gradle"], path_fragments: &[] },
        Rule { id: "cache.pip", category: Category::Cache, label: "Python package cache", explanation: "pip / uv / Poetry download cache; entries are re-downloaded on demand.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &["pip/cache", "uv/cache", "pypoetry/cache", "pip/ cache"] },
        Rule { id: "cache.generic", category: Category::Cache, label: "Cache", explanation: "Application or tool cache; contents are recreated automatically.", confidence: C, scope: RuleScope::Leaf, names: &["cache", "caches", ".cache", "code cache", "gpucache", "shadercache", "webcache", "cachedata"], path_fragments: &[] },
        Rule { id: "cache.cargo-registry", category: Category::Cache, label: "Cargo registry cache", explanation: "Downloaded crate sources and indexes; restored on demand by Cargo.", confidence: C, scope: RuleScope::Leaf, names: &[], path_fragments: &[".cargo/registry", ".cargo/git"] },
        // ---- Logs ----
        Rule { id: "logs.folder", category: Category::Logs, label: "Logs", explanation: "Log files written by applications and tools.", confidence: C, scope: RuleScope::Leaf, names: &["logs", "log", "crashpad", "crashdumps"], path_fragments: &[] },
        // ---- Developer tool homes (containers: deeper rules may refine) ----
        Rule { id: "dev.cargo-home", category: Category::DeveloperTools, label: "Cargo home", explanation: "Rust toolchain data, registry cache and installed binaries.", confidence: C, scope: RuleScope::Container, names: &[".cargo", ".rustup"], path_fragments: &[] },
        Rule { id: "dev.agents", category: Category::DeveloperTools, label: "AI coding agent data", explanation: "Configuration, sessions and caches for a local AI coding agent.", confidence: C, scope: RuleScope::Container, names: &[".claude", ".codex", ".copilot", ".gemini", ".cursor", ".continue"], path_fragments: &[] },
        Rule { id: "dev.vscode", category: Category::DeveloperTools, label: "Editor data", explanation: "Editor configuration, extensions and per-workspace state.", confidence: C, scope: RuleScope::Container, names: &[".vscode", ".vscode-insiders"], path_fragments: &["appdata/roaming/code", "appdata/roaming/cursor"] },
        Rule { id: "dev.models", category: Category::DeveloperTools, label: "Local AI models", explanation: "Locally stored AI model files and runtime data.", confidence: M, scope: RuleScope::Container, names: &[".ollama", ".lmstudio", ".cache/huggingface", "huggingface"], path_fragments: &[".ollama", ".lmstudio"] },
        // ---- User locations ----
        Rule { id: "user.downloads", category: Category::Downloads, label: "Downloads", explanation: "Files in the user Downloads folder.", confidence: C, scope: RuleScope::Container, names: &["downloads"], path_fragments: &[] },
        Rule { id: "user.data", category: Category::UserData, label: "User data", explanation: "Personal documents, pictures, music and desktop files.", confidence: M, scope: RuleScope::Container, names: &["documents", "pictures", "music", "videos", "desktop", "onedrive"], path_fragments: &[] },
        // ---- Application data (container) ----
        Rule { id: "app.data", category: Category::ApplicationData, label: "Application data", explanation: "Per-application settings and data stored under the user profile.", confidence: M, scope: RuleScope::Container, names: &["appdata"], path_fragments: &["appdata/local", "appdata/roaming", "library/application support", ".local/share", ".config"] },
        Rule { id: "app.local-share", category: Category::ApplicationData, label: "Application data", explanation: "Per-application data stored in the local share directory.", confidence: M, scope: RuleScope::Container, names: &[], path_fragments: &[".local/share", ".config/"] },
    ]
}

/// Extensions that refine a file inside a container to BinaryData.
pub fn binary_extensions() -> &'static [&'static str] {
    &[
        "gguf",
        "safetensors",
        "iso",
        "img",
        "vhdx",
        "vmdk",
        "qcow2",
        "zip",
        "7z",
        "rar",
        "tar",
        "gz",
        "bz2",
        "xz",
        "bin",
        "dat",
        "dmg",
        "msi",
        "exe",
        "dll",
        "so",
        "dylib",
        "onnx",
        "pt",
        "pth",
        "ckpt",
        "npy",
        "parquet",
    ]
}

pub fn log_extensions() -> &'static [&'static str] {
    &["log", "logs", "etl", "dmp"]
}

pub fn user_doc_extensions() -> &'static [&'static str] {
    &[
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "jpg", "jpeg", "png",
        "gif", "mp3", "mp4", "mov", "csv",
    ]
}
