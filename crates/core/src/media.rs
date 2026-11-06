//! Media totals over the scan index: how much of the disk is photos,
//! videos and screenshots, and the biggest files of each kind.
//!
//! Classification deliberately reuses the natural-language search
//! kind rules ([`crate::search::kind_ids_for`]) so the Media page and
//! search always agree about what a "photo" or a "video" is — same
//! extension tables, same screenshot name/folder rule.
//!
//! **Screenshot subset rule:** a screenshot is an image with the
//! extra `screenshot` kind, so screenshot files are counted in BOTH
//! the photos bucket and the screenshots bucket. The screenshots
//! bucket is a subset of photos, never an addition to it — the UI
//! labels it "of which screenshots". Bytes are the index's allocated
//! byte counts, per the product byte rule.

use crate::model::FileEntry;
use crate::search::kind_ids_for;
use serde::{Deserialize, Serialize};

/// How many entries each biggest list carries.
pub const BIGGEST_LIMIT: usize = 15;

/// Byte total (allocated) and file count for one media kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaBucket {
    pub bytes: u64,
    pub count: u64,
}

/// One file in a biggest list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub modified_ms: i64,
}

/// Media totals for one scan index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSummary {
    /// All image files (screenshots included — see the module docs).
    pub photos: MediaBucket,
    /// All video files.
    pub videos: MediaBucket,
    /// Image files that also match the screenshot rule; a subset of
    /// `photos`.
    pub screenshots: MediaBucket,
    /// The 15 largest videos, biggest first.
    pub biggest_videos: Vec<MediaItem>,
    /// The 15 largest photos (screenshots compete here too — they are
    /// photos), biggest first.
    pub biggest_photos: Vec<MediaItem>,
}

fn bucket_add(bucket: &mut MediaBucket, bytes: u64) {
    bucket.bytes = bucket.bytes.saturating_add(bytes);
    bucket.count += 1;
}

fn biggest(mut items: Vec<MediaItem>) -> Vec<MediaItem> {
    // Biggest first; path order breaks byte ties so the result is
    // deterministic for equal-sized files.
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    items.truncate(BIGGEST_LIMIT);
    items
}

/// Compute the media summary from a scan index's file entries. Pure:
/// the same entries always produce the same summary.
pub fn media_summary(files: &[FileEntry]) -> MediaSummary {
    let mut summary = MediaSummary::default();
    let mut photos: Vec<MediaItem> = Vec::new();
    let mut videos: Vec<MediaItem> = Vec::new();
    for f in files {
        let ids = kind_ids_for(&f.name, &f.path);
        if ids.contains(&"image") {
            bucket_add(&mut summary.photos, f.bytes);
            photos.push(MediaItem {
                path: f.path.clone(),
                name: f.name.clone(),
                bytes: f.bytes,
                modified_ms: f.modified_ms,
            });
            if ids.contains(&"screenshot") {
                bucket_add(&mut summary.screenshots, f.bytes);
            }
        }
        if ids.contains(&"video") {
            bucket_add(&mut summary.videos, f.bytes);
            videos.push(MediaItem {
                path: f.path.clone(),
                name: f.name.clone(),
                bytes: f.bytes,
                modified_ms: f.modified_ms,
            });
        }
    }
    summary.biggest_photos = biggest(photos);
    summary.biggest_videos = biggest(videos);
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    fn entry(name: &str, bytes: u64) -> FileEntry {
        FileEntry {
            path: format!("C:\\Users\\alex\\Pictures\\{name}"),
            name: name.to_string(),
            bytes,
            category: Category::UserData,
            modified_ms: 1_700_000_000_000,
            ocr_text: None,
        }
    }

    #[test]
    fn empty_index_is_all_zeroes() {
        let s = media_summary(&[]);
        assert_eq!(s.photos, MediaBucket::default());
        assert_eq!(s.videos, MediaBucket::default());
        assert_eq!(s.screenshots, MediaBucket::default());
        assert!(s.biggest_photos.is_empty());
        assert!(s.biggest_videos.is_empty());
    }

    #[test]
    fn kind_buckets_and_screenshot_subset() {
        let files = vec![
            entry("cat.jpg", 1_000),
            entry("dog.png", 2_000),
            entry("Screenshot 2026-11-06.png", 500),
            entry("clip.mp4", 9_000),
            entry("movie.mkv", 7_000),
            entry("notes.txt", 100),
            entry("song.mp3", 300),
        ];
        let s = media_summary(&files);
        // Photos: jpg + 2 png (screenshot included); videos: mp4 + mkv;
        // text and music count nowhere.
        assert_eq!(
            s.photos,
            MediaBucket {
                bytes: 3_500,
                count: 3
            }
        );
        assert_eq!(
            s.videos,
            MediaBucket {
                bytes: 16_000,
                count: 2
            }
        );
        // The screenshot is counted in photos AND in its subset bucket.
        assert_eq!(
            s.screenshots,
            MediaBucket {
                bytes: 500,
                count: 1
            }
        );
    }

    #[test]
    fn screenshot_by_folder_rule() {
        let mut in_folder = entry("IMG_2041.png", 800);
        in_folder.path = "C:\\Users\\alex\\Pictures\\Screenshots\\IMG_2041.png".to_string();
        let s = media_summary(&[in_folder]);
        assert_eq!(s.screenshots.count, 1);
        assert_eq!(s.photos.count, 1);
    }

    #[test]
    fn biggest_lists_are_top_15_by_bytes() {
        let mut files: Vec<FileEntry> = (0..20)
            .map(|i| entry(&format!("photo{i:02}.jpg"), (i as u64 + 1) * 100))
            .collect();
        files.push(entry("huge.mp4", 999_999));
        files.push(entry("small.mp4", 1));
        let s = media_summary(&files);
        assert_eq!(s.photos.count, 20);
        assert_eq!(s.biggest_photos.len(), BIGGEST_LIMIT);
        // Largest photo is photo19 (2000 B); the list is descending.
        assert_eq!(s.biggest_photos[0].name, "photo19.jpg");
        assert_eq!(s.biggest_photos[0].bytes, 2_000);
        assert!(s
            .biggest_photos
            .windows(2)
            .all(|w| w[0].bytes >= w[1].bytes));
        assert_eq!(s.biggest_videos.len(), 2);
        assert_eq!(s.biggest_videos[0].name, "huge.mp4");
    }
}
