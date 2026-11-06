//! Local AI model stores: Ollama (content-addressed blobs shared between
//! models) and LM Studio. Models are reference-only, except unused shared
//! layers and interrupted downloads, which the recommendation layer can
//! offer.

use super::{child_entries, file_name_str, make_item, DevPaths};
use crate::model::*;
use crate::scanner::measure_path;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct OllamaAnalysis {
    pub total_bytes: u64,
    pub shared_layer_bytes: u64,
    pub unused_layer_bytes: u64,
    pub interrupted_bytes: u64,
    pub model_count: usize,
}

pub fn analyze_ollama_at(models_dir: &Path) -> OllamaAnalysis {
    let mut result = OllamaAnalysis {
        total_bytes: measure_path(models_dir),
        ..Default::default()
    };
    let blobs = models_dir.join("blobs");
    let manifests_root = models_dir.join("manifests");
    // Count references to each blob digest across all manifests.
    let mut refs: HashMap<String, usize> = HashMap::new();
    collect_manifest_digests(&manifests_root, &mut refs);
    result.model_count = count_manifests(&manifests_root);
    if blobs.exists() {
        for entry in child_entries(&blobs) {
            let name = file_name_str(&entry);
            let bytes = measure_path(&entry);
            if name.ends_with("-partial") || name.contains("partial") {
                result.interrupted_bytes += bytes;
                continue;
            }
            let digest = name.trim_start_matches("sha256-");
            let digest_key = format!("sha256:{}", digest);
            let count = refs.get(&digest_key).copied().unwrap_or(0)
                + refs.get(&name).copied().unwrap_or(0)
                + refs.get(digest).copied().unwrap_or(0);
            if count >= 2 {
                result.shared_layer_bytes += bytes;
            } else if count == 0 {
                result.unused_layer_bytes += bytes;
            }
        }
    }
    result
}

fn count_manifests(dir: &Path) -> usize {
    let mut n = 0;
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let p = entry.path();
            if p.is_dir() {
                n += count_manifests(&p);
            } else {
                n += 1;
            }
        }
    }
    n
}

fn collect_manifest_digests(dir: &Path, refs: &mut HashMap<String, usize>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_manifest_digests(&p, refs);
        } else if let Ok(text) = std::fs::read_to_string(&p) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                collect_digests_from_value(&value, refs);
            }
        }
    }
}

fn collect_digests_from_value(value: &serde_json::Value, refs: &mut HashMap<String, usize>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if k == "digest" {
                    if let Some(s) = v.as_str() {
                        *refs.entry(s.to_string()).or_insert(0) += 1;
                    }
                }
                collect_digests_from_value(v, refs);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_digests_from_value(v, refs);
            }
        }
        _ => {}
    }
}

pub fn analyze(paths: &DevPaths) -> Option<ToolReport> {
    let mut sections: Vec<ToolSection> = Vec::new();
    let ollama_models = paths.home.join(".ollama").join("models");
    if ollama_models.exists() {
        let a = analyze_ollama_at(&ollama_models);
        let mut items = vec![ToolItem {
            name: format!("Ollama models ({} model(s))", a.model_count),
            path: ollama_models.to_string_lossy().to_string(),
            bytes: a.total_bytes,
            state: ToolItemState::Informational,
            detail: "Local models are reference only; deleting a model is done from Ollama itself."
                .to_string(),
        }];
        if a.shared_layer_bytes > 0 {
            items.push(ToolItem {
                name: "Shared layers".to_string(),
                path: ollama_models.join("blobs").to_string_lossy().to_string(),
                bytes: a.shared_layer_bytes,
                state: ToolItemState::Informational,
                detail: "Blobs referenced by more than one model; stored once.".to_string(),
            });
        }
        if a.unused_layer_bytes > 0 {
            items.push(ToolItem {
                name: "Unused layers".to_string(),
                path: ollama_models.join("blobs").to_string_lossy().to_string(),
                bytes: a.unused_layer_bytes,
                state: ToolItemState::Stale,
                detail: "Blobs not referenced by any installed model manifest.".to_string(),
            });
        }
        if a.interrupted_bytes > 0 {
            items.push(ToolItem {
                name: "Interrupted downloads".to_string(),
                path: ollama_models.join("blobs").to_string_lossy().to_string(),
                bytes: a.interrupted_bytes,
                state: ToolItemState::Stale,
                detail: "Partial blob downloads that never completed.".to_string(),
            });
        }
        sections.push(ToolSection {
            title: "Ollama".to_string(),
            items,
        });
    }

    let lmstudio = paths.home.join(".lmstudio").join("models");
    if lmstudio.exists() {
        let mut items = Vec::new();
        for publisher in super::child_dirs(&lmstudio) {
            for model in super::child_dirs(&publisher) {
                items.push(make_item(
                    format!("{}/{}", file_name_str(&publisher), file_name_str(&model)),
                    &model,
                    ToolItemState::Informational,
                    "LM Studio model; reference only.",
                ));
            }
        }
        if !items.is_empty() {
            sections.push(ToolSection {
                title: "LM Studio".to_string(),
                items,
            });
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(ToolReport {
            tool: "Local AI models".to_string(),
            sections,
        })
    }
}
