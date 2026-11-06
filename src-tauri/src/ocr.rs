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

/// How many recognized texts the cache keeps. When a pass pushes
/// it past the cap, the least-recently-inserted entries are evicted
/// (see the `OcrCache` note) — recent screenshots are the ones users
/// search for, and their text is the last worth losing.
const MAX_CACHE_ENTRIES: usize = 5_000;

/// Recognized-text cache: (core cache key, recognized text) pairs
/// IN INSERTION ORDER. A Vec rather than a map so the eviction
/// order above is real, and so the disk format — a JSON array of
/// pairs — preserves it across runs. (Caches written by earlier
/// versions were a JSON object; loading still accepts those, their
/// order simply becomes whatever the old file held.) Only positive
/// results are cached — a file with no text (or an engine without a
/// language pack that day) is retried on a later pass rather than
/// being pinned as permanently textless.
type OcrCache = Vec<(String, String)>;

fn load_cache(path: &Path) -> OcrCache {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if let Ok(pairs) = serde_json::from_str::<OcrCache>(&text) {
        return pairs;
    }
    // Legacy format: a plain key → text object.
    serde_json::from_str::<HashMap<String, String>>(&text)
        .map(|map| map.into_iter().collect())
        .unwrap_or_default()
}

fn cache_get<'a>(cache: &'a OcrCache, key: &str) -> Option<&'a str> {
    cache
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, text)| text.as_str())
}

fn cache_insert(cache: &mut OcrCache, key: String, text: String) {
    if let Some(slot) = cache.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = text;
    } else {
        cache.push((key, text));
    }
}

/// The file path inside a core cache key (`path|modifiedMs|bytes`).
/// Parsed from the right: Windows file names cannot contain `|`,
/// so the last two `|`-separated fields are always the mtime and
/// size (and on other hosts a `|` inside a name still parses
/// correctly, since only trailing fields are stripped).
fn cache_key_path(key: &str) -> Option<&str> {
    let (head, _bytes) = key.rsplit_once('|')?;
    let (path, _modified_ms) = head.rsplit_once('|')?;
    Some(path)
}

/// Drop cache entries whose file no longer exists (deleted or
/// moved since it was recognized). Runs at load, once per pass;
/// each check is one metadata call.
fn prune_missing_files(cache: &mut OcrCache) {
    cache.retain(|(key, _)| {
        cache_key_path(key)
            .map(|path| Path::new(path).exists())
            .unwrap_or(false)
    });
}

/// Evict least-recently-inserted entries until the cache is within
/// its cap.
fn evict_over_cap(cache: &mut OcrCache) {
    if cache.len() > MAX_CACHE_ENTRIES {
        cache.drain(..cache.len() - MAX_CACHE_ENTRIES);
    }
}

fn save_cache(path: &Path, cache: &OcrCache, generation: u64) {
    // Written via a temp file so a crash mid-write cannot
    // corrupt the previous cache (same pattern as settings
    // persistence). The temp name is unique to this pass
    // (generation-suffixed), so two passes writing around the same
    // moment never share a temp path.
    let Ok(text) = serde_json::to_string(cache) else {
        return;
    };
    let tmp = path.with_extension(format!("json.tmp{generation}"));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Write recognized text into the index entries of the engine
/// state, in place. `texts` is keyed by the core cache key
/// (path + mtime + size), and each entry is matched by ITS key —
/// never by path alone — so a file that changed since it was
/// recognized (different key) is not decorated with another
/// version's text. A key match REPLACES the entry's text: the key
/// identifies the exact bytes the text was read from, and a newer
/// pass's reading of those same bytes supersedes an older one.
fn apply_text(engine: &mut AppState, texts: &HashMap<String, String>) {
    let Some(index) = engine.index.as_mut() else {
        return;
    };
    for file in &mut index.files {
        let key = ocr::cache_key(&file.path, file.modified_ms, file.bytes);
        if let Some(text) = texts.get(&key) {
            file.ocr_text = Some(text.clone());
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
        // Hygiene before anything reads the cache: entries for
        // files that no longer exist are dead weight (and their
        // keys could collide with a future file reusing the path).
        prune_missing_files(&mut cache);

        // Cache hits are applied first, so search sees previously
        // recognized text immediately; only misses cost engine time.
        let mut misses: Vec<OcrCandidate> = Vec::new();
        let mut hits: HashMap<String, String> = HashMap::new();
        for candidate in candidates {
            match cache_get(&cache, &candidate.cache_key) {
                Some(text) => {
                    hits.insert(candidate.cache_key.clone(), text.to_string());
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
        let worker_a =
            tauri::async_runtime::spawn_blocking(move || run_shard(misses, &counter_a, generation));
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
        let mut by_key: HashMap<String, String> = HashMap::new();
        for (key, _path, text) in fresh {
            cache_insert(&mut cache, key.clone(), text.clone());
            by_key.insert(key, text);
        }
        evict_over_cap(&mut cache);
        save_cache(&cache_file, &cache, generation);
        if let Ok(mut engine) = state.engine.lock() {
            // The generation is re-checked WHILE HOLDING the engine
            // lock. A newer scan bumps the generation when it starts
            // — before it can store anything — and storing takes this
            // same lock, so under the lock the check is decisive: a
            // mismatch means a newer scan owns (or is about to own)
            // the index and this pass must not touch it; a match
            // means any scan starting later replaces the whole index
            // wholesale after this apply, discarding it harmlessly.
            // Checking before locking left a window where a scan
            // could store a fresh index and this pass would then
            // decorate it with the previous scan's text.
            if generation_counter.load(Ordering::SeqCst) == generation {
                apply_text(&mut engine, &by_key);
            }
        };
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
        cache_insert(&mut cache, "a.png|1|2".to_string(), "hello".to_string());
        save_cache(&path, &cache, 3);
        assert_eq!(load_cache(&path), cache);
        assert!(
            !path.with_extension("json.tmp3").exists(),
            "the per-pass temp file is renamed away"
        );

        std::fs::write(&path, "{not json").unwrap();
        assert!(load_cache(&path).is_empty(), "damaged file → empty");
    }

    #[test]
    fn cache_still_loads_the_legacy_object_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ocr_cache.json");
        std::fs::write(&path, r#"{"a.png|1|2": "hello"}"#).unwrap();
        let cache = load_cache(&path);
        assert_eq!(cache_get(&cache, "a.png|1|2"), Some("hello"));
    }

    #[test]
    fn cache_prunes_dead_files_and_evicts_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let alive = dir.path().join("alive.png");
        std::fs::write(&alive, b"x").unwrap();
        let alive_key = format!("{}|1|2", alive.to_string_lossy());
        let mut cache = OcrCache::new();
        cache_insert(&mut cache, "gone.png|1|2".to_string(), "dead".to_string());
        cache_insert(&mut cache, alive_key.clone(), "live".to_string());
        prune_missing_files(&mut cache);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache_get(&cache, &alive_key), Some("live"));

        let mut big: OcrCache = (0..MAX_CACHE_ENTRIES + 3)
            .map(|i| (format!("f{i}.png|1|2"), format!("text {i}")))
            .collect();
        evict_over_cap(&mut big);
        assert_eq!(big.len(), MAX_CACHE_ENTRIES);
        assert_eq!(
            big[0].0, "f3.png|1|2",
            "the least-recently-inserted entries go first"
        );
    }

    #[test]
    fn apply_text_matches_by_cache_key_and_replaces_on_key_match() {
        let mut engine = AppState::new();
        engine.index = Some(DirIndex {
            files: vec![
                entry("/root/a.png", None),
                entry("/root/b.png", Some("stale")),
                entry("/root/c.png", None),
            ],
            ..Default::default()
        });
        let key = |p: &str| ocr::cache_key(p, 0, 10);
        let texts: HashMap<String, String> = [
            (key("/root/a.png"), "fresh".to_string()),
            (key("/root/b.png"), "replacement".to_string()),
            // Text keyed for a DIFFERENT version of c.png (mtime 9):
            // it must not land on the current entry.
            (
                ocr::cache_key("/root/c.png", 9, 10),
                "wrong version".to_string(),
            ),
        ]
        .into_iter()
        .collect();
        apply_text(&mut engine, &texts);
        let files = &engine.index.as_ref().unwrap().files;
        assert_eq!(files[0].ocr_text.as_deref(), Some("fresh"));
        assert_eq!(
            files[1].ocr_text.as_deref(),
            Some("replacement"),
            "a key-matched newer reading replaces the older one"
        );
        assert_eq!(files[2].ocr_text, None, "no key match, no text");
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
