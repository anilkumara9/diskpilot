//! On-device text recognition for scanned images (screenshots and
//! pictures), backed by Windows' built-in OCR engine
//! (`Windows.Media.Ocr`). The recognized text lands on
//! `FileEntry::ocr_text` via the shell's post-scan pass, which is
//! what lets file search match words that appear only inside a
//! picture.
//!
//! The WinRT surface is confined to this module and compiles only
//! on Windows; everywhere else [`ocr_image`] returns
//! [`OcrError::Unsupported`] — never fabricated text. Candidate
//! selection and the cache key are pure and platform-free.

use crate::model::FileEntry;
use crate::search::kind_ids_for;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OcrError {
    /// OCR needs Windows' built-in engine; there is no fallback.
    #[error("OCR is only available on Windows")]
    Unsupported,
    /// The image file itself could not be read.
    #[error("could not read the image: {0}")]
    Io(String),
    /// The engine or the imaging stack failed on a readable file.
    #[error("the OCR engine failed: {0}")]
    Engine(String),
}

pub type OcrResult<T> = Result<T, OcrError>;

/// Largest image (allocated bytes) a pass will attempt. Bigger
/// files are skipped, not failed — recognition cost grows with
/// pixels, and screenshots are small.
pub const MAX_OCR_FILE_BYTES: u64 = 20_000_000;

/// Most files one pass will consider, newest first.
pub const MAX_OCR_FILES_PER_PASS: usize = 400;

/// Extensions the Windows imaging stack can decode. WebP is
/// included: it decodes when the OS codec is present, and a decode
/// failure is a per-file engine error the pass skips — never a
/// pass failure.
const OCR_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"];

/// One file the OCR pass should consider, with the key its result
/// is cached under in the shell's `ocr_cache.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrCandidate {
    pub path: String,
    pub cache_key: String,
}

/// The cache key for one index entry: path + mtime + size identify
/// the file's current bytes, so an edited or replaced image is
/// recognized again and an untouched one never is.
pub fn cache_key(path: &str, modified_ms: i64, bytes: u64) -> String {
    format!("{path}|{modified_ms}|{bytes}")
}

/// Pick the files worth recognizing: entries the search grammar
/// calls an image or a screenshot, with a decodable extension and
/// a plausible size. Newest first — recent screenshots are the
/// ones users search for — capped at
/// [`MAX_OCR_FILES_PER_PASS`].
pub fn select_candidates(files: &[FileEntry]) -> Vec<OcrCandidate> {
    let mut picked: Vec<&FileEntry> = files.iter().filter(|f| is_ocr_candidate(f)).collect();
    picked.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| a.path.cmp(&b.path))
    });
    picked.truncate(MAX_OCR_FILES_PER_PASS);
    picked
        .into_iter()
        .map(|f| OcrCandidate {
            path: f.path.clone(),
            cache_key: cache_key(&f.path, f.modified_ms, f.bytes),
        })
        .collect()
}

fn is_ocr_candidate(f: &FileEntry) -> bool {
    if f.bytes == 0 || f.bytes > MAX_OCR_FILE_BYTES {
        return false;
    }
    let lower = f.name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if !OCR_EXTENSIONS.contains(&ext) {
        return false;
    }
    let kinds = kind_ids_for(&f.name, &f.path);
    kinds.contains(&"image") || kinds.contains(&"screenshot")
}

/// Recognize the text in one image file.
///
/// `Ok(Some(text))` — text was found. `Ok(None)` — the engine is
/// unavailable for lack of a recognizer language pack, or the image
/// carries no text. `Err` — a real failure (unreadable file, engine
/// or decoder error).
#[cfg(windows)]
pub fn ocr_image(path: &Path) -> OcrResult<Option<String>> {
    windows_impl::ocr_image(path)
}

/// Off Windows there is no engine to call: a typed refusal, never
/// invented text.
#[cfg(not(windows))]
pub fn ocr_image(_path: &Path) -> OcrResult<Option<String>> {
    Err(OcrError::Unsupported)
}

#[cfg(windows)]
mod windows_impl {
    use super::{OcrError, OcrResult};
    use std::path::Path;
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat,
        BitmapTransform, ColorManagementMode, ExifOrientationMode, SoftwareBitmap,
    };
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    fn engine_err(e: windows::core::Error) -> OcrError {
        OcrError::Engine(e.to_string())
    }

    pub fn ocr_image(path: &Path) -> OcrResult<Option<String>> {
        // No recognizer language installed → no OCR text anywhere,
        // reported as "no text", not as a failure.
        let languages = OcrEngine::AvailableRecognizerLanguages().map_err(engine_err)?;
        if languages.Size().map_err(engine_err)? == 0 {
            return Ok(None);
        }
        let engine = match OcrEngine::TryCreateFromUserProfileLanguages() {
            Ok(engine) => engine,
            // The user's profile languages have no matching pack.
            Err(_) => return Ok(None),
        };

        let bytes = std::fs::read(path).map_err(|e| OcrError::Io(e.to_string()))?;
        let stream = InMemoryRandomAccessStream::new().map_err(engine_err)?;
        {
            let writer = DataWriter::CreateDataWriter(&stream).map_err(engine_err)?;
            writer.WriteBytes(&bytes).map_err(engine_err)?;
            writer.StoreAsync().map_err(engine_err)?.get().map_err(engine_err)?;
            writer.FlushAsync().map_err(engine_err)?.get().map_err(engine_err)?;
            writer.DetachStream().map_err(engine_err)?;
        }
        stream.Seek(0).map_err(engine_err)?;

        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(engine_err)?
            .get()
            .map_err(engine_err)?;
        let max_dim = OcrEngine::MaxImageDimension().map_err(engine_err)?;
        let bitmap = decode_for_engine(&decoder, max_dim)?;

        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(engine_err)?
            .get()
            .map_err(engine_err)?;
        let text = result.Text().map_err(engine_err)?.to_string();
        let text = text.trim();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text.to_string()))
        }
    }

    /// Decode to the BGRA8 bitmap the engine consumes, scaling down
    /// through the decoder when the image exceeds the engine's
    /// dimension limit (multi-monitor screenshots can).
    fn decode_for_engine(decoder: &BitmapDecoder, max_dim: u32) -> OcrResult<SoftwareBitmap> {
        let width = decoder.PixelWidth().map_err(engine_err)?;
        let height = decoder.PixelHeight().map_err(engine_err)?;
        let longest = width.max(height);
        if max_dim == 0 || longest <= max_dim {
            return decoder
                .GetSoftwareBitmapConvertedAsync(
                    BitmapPixelFormat::Bgra8,
                    BitmapAlphaMode::Premultiplied,
                )
                .map_err(engine_err)?
                .get()
                .map_err(engine_err);
        }
        let scale = max_dim as f64 / longest as f64;
        let transform = BitmapTransform::new().map_err(engine_err)?;
        transform
            .SetScaledWidth(((width as f64 * scale) as u32).max(1))
            .map_err(engine_err)?;
        transform
            .SetScaledHeight(((height as f64 * scale) as u32).max(1))
            .map_err(engine_err)?;
        transform
            .SetInterpolationMode(BitmapInterpolationMode::Linear)
            .map_err(engine_err)?;
        decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &transform,
                ExifOrientationMode::IgnoreExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )
            .map_err(engine_err)?
            .get()
            .map_err(engine_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    fn entry(name: &str, bytes: u64, modified_ms: i64) -> FileEntry {
        FileEntry {
            path: format!("/root/{name}"),
            name: name.to_string(),
            bytes,
            category: Category::Unknown,
            modified_ms,
            ocr_text: None,
        }
    }

    #[test]
    fn cache_key_combines_path_mtime_and_size() {
        assert_eq!(
            cache_key("C:\\a b.png", 1_700_000_000_000, 123),
            "C:\\a b.png|1700000000000|123"
        );
        // Any change to the file's identity changes the key.
        assert_ne!(cache_key("C:\\a.png", 1, 10), cache_key("C:\\a.png", 2, 10));
        assert_ne!(cache_key("C:\\a.png", 1, 10), cache_key("C:\\a.png", 1, 11));
    }

    #[test]
    fn candidates_are_images_within_size_and_extension_bounds() {
        let files = vec![
            entry("photo.png", 1_000, 10),
            entry("Screenshot 2024.jpg", 2_000, 30),
            entry("clip.mp4", 1_000, 40),              // video kind, wrong extension
            entry("notes.png.txt", 1_000, 50),         // image bytes, text extension
            entry("empty.png", 0, 60),                 // zero bytes
            entry("huge.png", MAX_OCR_FILE_BYTES + 1, 70), // over the cap
            entry("max.png", MAX_OCR_FILE_BYTES, 20),  // exactly at the cap
            entry("diagram.webp", 3_000, 15),
        ];
        let picked = select_candidates(&files);
        let paths: Vec<&str> = picked.iter().map(|c| c.path.as_str()).collect();
        // Newest first; only the four qualifying images.
        assert_eq!(
            paths,
            vec![
                "/root/Screenshot 2024.jpg",
                "/root/max.png",
                "/root/diagram.webp",
                "/root/photo.png",
            ]
        );
        assert_eq!(
            picked[0].cache_key,
            cache_key("/root/Screenshot 2024.jpg", 30, 2_000)
        );
    }

    #[test]
    fn candidates_cap_at_the_pass_limit_newest_first() {
        let files: Vec<FileEntry> = (0..(MAX_OCR_FILES_PER_PASS as i64 + 50))
            .map(|i| entry(&format!("img{i}.png"), 100, i))
            .collect();
        let picked = select_candidates(&files);
        assert_eq!(picked.len(), MAX_OCR_FILES_PER_PASS);
        // The newest file overall is first; the oldest picked is
        // exactly at the cap boundary.
        assert!(picked[0].path.ends_with(&format!(
            "img{}.png",
            MAX_OCR_FILES_PER_PASS + 49
        )));
    }

    #[cfg(not(windows))]
    #[test]
    fn ocr_image_off_windows_is_a_typed_unsupported() {
        let err = ocr_image(Path::new("/nonexistent.png")).unwrap_err();
        assert!(matches!(err, OcrError::Unsupported));
    }
}
