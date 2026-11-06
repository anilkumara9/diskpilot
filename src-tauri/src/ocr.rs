//! Post-scan OCR pass: shell orchestration around the core's
//! on-device recognizer (`diskpilot_core::ocr`).
//!
//! After a scan completes, images and screenshots from the fresh
//! index are recognized in the background — never blocking the scan
//! result — and the text is written back into the in-memory index
//! entries, where file search picks it up. Results are cached in
//! `ocr_cache.json` keyed by path + mtime + size, so an unchanged
//! image is never recognized twice. At most two images are in
//! flight at once, and a new scan stops the pass cooperatively via
//! the shell's OCR generation counter.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{AppHandle, Manager};

use diskpilot_core::ocr::{self, OcrCandidate, OcrError};
use diskpilot_core::AppState;

use crate::ShellState;

/// Recognized-text cache: core cache key → recognized text. Only
/// positive results are cached — a file with no text (or an engine
/// without a language pack that day) is retried on a later pass
/// rather than being pinned as permanently textless.
type OcrCache = HashMap<String, String>;

fn load_cache(path: &Path) -> OcrCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_cache(path: &Path, cache: &OcrCache) {
    // Written via a temp file so a crash mid-write cannot corrupt
    // the previous cache (same pattern as settings persistence).
    let Ok(text) = serde_json::to_string(cache) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Write recognized text (path → text) into the index entries of
/// the engine state, in place. Entries without a text keep
/// whatever they had (normally nothing).
fn apply_text(engine: &mut AppState, texts: &HashMap<String, String>) {
    let Some(index) = engine.index.as_mut() else {
        return;
    };
    for file in &mut index.files {
        if file.ocr_text.is_none() {
            if let Some(text) = texts.get(&file.path) {
                file.ocr_text = Some(text.clone());
            }
        }
    }
}

/// One shard of the recognition work, run on a blocking thread:
/// recognizes each candidate in order, checking the generation
/// between files so a newer scan stops the pass promptly. Returns
/// (cache key, path, text) for every positive result.
fn run_shard(
    candidates: Vec<OcrCandidate>,
    generation_counter: &AtomicU64,
    generation: u64,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for candidate in candidates {
        if generation_counter.load(Ordering::SeqCst) != generation {
            break;
        }
        match ocr::ocr_image(Path::new(&candidate.path)) {
            Ok(Some(text)) => out.push((candidate.cache_key, candidate.path, text)),
            // No text in the image (or no language pack): nothing
            // to store.
            Ok(None) => {}
            // The platform has no OCR at all: stop the whole pass
            // instead of failing every remaining file the same way.
            Err(OcrError::Unsupported) => break,
            // Unreadable file, decoder gap (e.g. WebP without the
            // OS codec), engine hiccup: per-file, non-fatal.
            Err(_) => {}
        }
    }
    out
}

/// Spawn the post-scan OCR pass for the scan generation that just
/// completed. A no-op when OCR is disabled in settings or the
/// completed scan left no index.
pub fn spawn_pass(app: AppHandle, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<ShellState>();
        let enabled = state
            .settings
            .lock()
            .map(|settings| settings.ocr_enabled)
            .unwrap_or(false);
        if !enabled {
            return;
        }

        let files = {
            let Ok(engine) = state.engine.lock() else {
                return;
            };
            match &engine.index {
                Some(index) => index.files.clone(),
                None => return,
            }
        };
        let candidates = ocr::select_candidates(&files);
        if candidates.is_empty() {
            return;
        }

        let cache_file: PathBuf = state.ocr_cache_file();
        let mut cache = load_cache(&cache_file);

        // Cache hits are applied first, so search sees previously
        // recognized text immediately; only misses cost engine time.
        let mut misses: Vec<OcrCandidate> = Vec::new();
        let mut hits: HashMap<String, String> = HashMap::new();
        for candidate in candidates {
            match cache.get(&candidate.cache_key) {
                Some(text) => {
                    hits.insert(candidate.path.clone(), text.clone());
                }
                None => misses.push(candidate),
            }
        }
        if !hits.is_empty() {
            if let Ok(mut engine) = state.engine.lock() {
                let generation_now = state.ocr_generation.load(Ordering::SeqCst);
                if generation_now == generation {
                    apply_text(&mut engine, &hits);
                }
            }
        }
        if misses.is_empty() {
            return;
        }

        // Two blocking workers over two shards: at most two images
        // are being recognized at any moment.
        let half = misses.len().div_ceil(2);
        let shard_b: Vec<OcrCandidate> = misses.drain(half..).collect();
        let generation_counter = state.ocr_generation.clone();
        let counter_a = generation_counter.clone();
        let worker_a = tauri::async_runtime::spawn_blocking(move || {
            run_shard(misses, &counter_a, generation)
        });
        let counter_b = generation_counter.clone();
        let worker_b = tauri::async_runtime::spawn_blocking(move || {
            run_shard(shard_b, &counter_b, generation)
        });
        let mut fresh: Vec<(String, String, String)> = Vec::new();
        for worker in [worker_a, worker_b] {
            if let Ok(mut results) = worker.await {
                fresh.append(&mut results);
            }
        }
        if fresh.is_empty() {
            return;
        }

        // Cache results even when a newer scan superseded this pass
        // mid-flight (the keys stay valid); write into the index
        // only when this pass still belongs to the current scan.
        let mut by_path: HashMap<String, String> = HashMap::new();
        for (key, path, text) in fresh {
            cache.insert(key, text.clone());
            by_path.insert(path, text);
        }
        save_cache(&cache_file, &cache);
        if generation_counter.load(Ordering::SeqCst) == generation {
            if let Ok(mut engine) = state.engine.lock() {
                apply_text(&mut engine, &by_path);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use diskpilot_core::model::{Category, FileEntry};
    use diskpilot_core::scanner::DirIndex;

    fn entry(path: &str, ocr_text: Option<&str>) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            bytes: 10,
            category: Category::Unknown,
            modified_ms: 0,
            ocr_text: ocr_text.map(|s| s.to_string()),
        }
    }

    #[test]
    fn cache_round_trips_and_tolerates_damage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ocr_cache.json");
        assert!(load_cache(&path).is_empty(), "missing file → empty");

        let mut cache = OcrCache::new();
        cache.insert("a.png|1|2".to_string(), "hello".to_string());
        save_cache(&path, &cache);
        assert_eq!(load_cache(&path), cache);

        std::fs::write(&path, "{not json").unwrap();
        assert!(load_cache(&path).is_empty(), "damaged file → empty");
    }

    #[test]
    fn apply_text_fills_only_entries_without_text() {
        let mut engine = AppState::new();
        engine.index = Some(DirIndex {
            files: vec![
                entry("/root/a.png", None),
                entry("/root/b.png", Some("already")),
                entry("/root/c.png", None),
            ],
            ..Default::default()
        });
        let texts: HashMap<String, String> = [
            ("/root/a.png".to_string(), "fresh".to_string()),
            ("/root/b.png".to_string(), "replacement".to_string()),
        ]
        .into_iter()
        .collect();
        apply_text(&mut engine, &texts);
        let files = &engine.index.as_ref().unwrap().files;
        assert_eq!(files[0].ocr_text.as_deref(), Some("fresh"));
        assert_eq!(
            files[1].ocr_text.as_deref(),
            Some("already"),
            "existing text is never overwritten"
        );
        assert_eq!(files[2].ocr_text, None);
    }

    #[test]
    fn shard_stops_when_the_generation_moves_on() {
        let counter = AtomicU64::new(7);
        let candidates = vec![OcrCandidate {
            path: "/root/a.png".to_string(),
            cache_key: "k".to_string(),
        }];
        // Generation 6 != current 7: the shard stops before touching
        // the (here nonexistent) file at all.
        let out = run_shard(candidates, &counter, 6);
        assert!(out.is_empty());
    }
}
