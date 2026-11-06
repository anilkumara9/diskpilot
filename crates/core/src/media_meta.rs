//! Media metadata from file headers — dimensions for PNG, JPEG, GIF, BMP
//! and WebP, plus camera make/model and capture time from JPEG EXIF.
//!
//! Everything is parsed by hand from the first bytes of the file: no image
//! decoding, no dependencies, and every read is bounds-checked so a
//! truncated or hostile file yields `None` (or partial info), never a
//! panic. EXIF timestamps carry no timezone; they are interpreted as UTC.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Header-level facts about one image file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Camera make from EXIF (JPEG only).
    pub make: Option<String>,
    /// Camera model from EXIF (JPEG only).
    pub model: Option<String>,
    /// Capture time from EXIF DateTimeOriginal (or DateTime), ms since the
    /// Unix epoch, interpreting the naive EXIF timestamp as UTC.
    pub taken_at_ms: Option<i64>,
}

/// How much of the head of a file the parsers get to see. EXIF and all
/// dimension headers live at the start of the file.
const HEAD_BYTES: u64 = 1 << 20;

/// Read the header facts for one image file. `None` when the file cannot
/// be read or none of its headers parse; a `Some` may still carry all-None
/// fields when only the container signature matched.
pub fn media_info(path: &Path) -> Option<MediaInfo> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.by_ref().take(HEAD_BYTES).read_to_end(&mut buf).ok()?;
    media_info_from_bytes(&buf)
}

/// Byte-slice core of [`media_info`], split out for testing.
pub fn media_info_from_bytes(buf: &[u8]) -> Option<MediaInfo> {
    if buf.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return png_info(buf);
    }
    if buf.starts_with(b"GIF87a") || buf.starts_with(b"GIF89a") {
        return gif_info(buf);
    }
    if buf.starts_with(b"BM") {
        return bmp_info(buf);
    }
    if buf.len() >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP" {
        return webp_info(buf);
    }
    if buf.starts_with(&[0xFF, 0xD8]) {
        return jpeg_info(buf);
    }
    None
}

fn empty() -> MediaInfo {
    MediaInfo {
        width: None,
        height: None,
        make: None,
        model: None,
        taken_at_ms: None,
    }
}

fn be_u32(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(buf.get(at..at + 4)?.try_into().ok()?))
}
fn le_u16(buf: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(buf.get(at..at + 2)?.try_into().ok()?))
}
fn le_u32(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buf.get(at..at + 4)?.try_into().ok()?))
}

fn png_info(buf: &[u8]) -> Option<MediaInfo> {
    // 8-byte signature, then the IHDR chunk: len(4) "IHDR" w(4) h(4).
    if buf.get(12..16)? != b"IHDR" {
        return Some(empty());
    }
    let mut info = empty();
    info.width = be_u32(buf, 16);
    info.height = be_u32(buf, 20);
    Some(info)
}

fn gif_info(buf: &[u8]) -> Option<MediaInfo> {
    let mut info = empty();
    info.width = le_u16(buf, 6).map(u32::from);
    info.height = le_u16(buf, 8).map(u32::from);
    Some(info)
}

fn bmp_info(buf: &[u8]) -> Option<MediaInfo> {
    let mut info = empty();
    match le_u32(buf, 14)? {
        12 => {
            // BITMAPCOREHEADER: u16 dimensions.
            info.width = le_u16(buf, 18).map(u32::from);
            info.height = le_u16(buf, 20).map(u32::from);
        }
        n if n >= 40 => {
            // BITMAPINFOHEADER and later: i32 dimensions; a negative
            // height means a top-down bitmap.
            info.width = le_u32(buf, 18).and_then(|v| u32::try_from(v as i32).ok());
            info.height = le_u32(buf, 22).map(|v| (v as i32).unsigned_abs());
        }
        _ => {}
    }
    Some(info)
}

fn webp_info(buf: &[u8]) -> Option<MediaInfo> {
    let mut info = empty();
    match buf.get(12..16)? {
        b"VP8X" => {
            // flags(1) reserved(3) then 24-bit (width-1), (height-1).
            let w = buf.get(24..27)?;
            let h = buf.get(27..30)?;
            info.width = Some(u32::from_le_bytes([w[0], w[1], w[2], 0]) + 1);
            info.height = Some(u32::from_le_bytes([h[0], h[1], h[2], 0]) + 1);
        }
        b"VP8L" => {
            // signature byte 0x2F, then 14 bits (width-1) + 14 bits (height-1).
            if *buf.get(20)? != 0x2F {
                return Some(info);
            }
            let v = le_u32(buf, 21)?;
            info.width = Some((v & 0x3FFF) + 1);
            info.height = Some(((v >> 14) & 0x3FFF) + 1);
        }
        b"VP8 " => {
            // frame tag(3) start code(3) then 14-bit dimensions.
            let w = le_u16(buf, 26)?;
            let h = le_u16(buf, 28)?;
            info.width = Some(u32::from(w & 0x3FFF));
            info.height = Some(u32::from(h & 0x3FFF));
        }
        _ => {}
    }
    Some(info)
}

// ---------------------------------------------------------------- JPEG ----

struct TiffReader<'a> {
    buf: &'a [u8],
    little: bool,
}

impl<'a> TiffReader<'a> {
    fn u16(&self, at: usize) -> Option<u16> {
        let b = self.buf.get(at..at + 2)?;
        Some(if self.little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }
    fn u32(&self, at: usize) -> Option<u32> {
        let b = self.buf.get(at..at + 4)?;
        Some(if self.little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    }
    /// Raw bytes of an IFD entry's value, resolving the inline-vs-offset
    /// rule. Offsets are TIFF-header relative.
    fn entry_value(&self, entry_at: usize, ty: u16, count: u32) -> Option<&'a [u8]> {
        let unit: usize = match ty {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        let total = (count as usize).checked_mul(unit)?;
        if total <= 4 {
            self.buf.get(entry_at + 8..entry_at + 8 + total)
        } else {
            let off = self.u32(entry_at + 8)? as usize;
            self.buf.get(off..off + total)
        }
    }
    fn ascii(&self, entry_at: usize, ty: u16, count: u32) -> Option<String> {
        if ty != 2 {
            return None;
        }
        let bytes = self.entry_value(entry_at, ty, count)?;
        let text: String = bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        let text = text.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
    /// Iterate the entries of the IFD at `ifd_off`, calling `f(tag, entry_at,
    /// type, count)`; a `false` return stops the walk.
    fn walk_ifd(&self, ifd_off: usize, mut f: impl FnMut(u16, usize, u16, u32) -> bool) {
        let Some(count) = self.u16(ifd_off) else {
            return;
        };
        for i in 0..count as usize {
            let at = ifd_off + 2 + i * 12;
            let (Some(tag), Some(ty), Some(cnt)) =
                (self.u16(at), self.u16(at + 2), self.u32(at + 4))
            else {
                return;
            };
            if !f(tag, at, ty, cnt) {
                return;
            }
        }
    }
}

fn parse_exif(tiff: &[u8]) -> (Option<String>, Option<String>, Option<i64>) {
    let (mut make, mut model, mut taken) = (None, None, None);
    let little = match tiff.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return (make, model, taken),
    };
    let r = TiffReader { buf: tiff, little };
    if r.u16(2) != Some(42) {
        return (make, model, taken);
    }
    let Some(ifd0) = r.u32(4).map(|v| v as usize) else {
        return (make, model, taken);
    };
    let mut exif_ifd: Option<usize> = None;
    r.walk_ifd(ifd0, |tag, at, ty, cnt| {
        match tag {
            0x010F => make = r.ascii(at, ty, cnt),
            0x0110 => model = r.ascii(at, ty, cnt),
            0x0132 => taken = r.ascii(at, ty, cnt).and_then(|s| parse_exif_datetime(&s)),
            0x8769 => {
                if let Some(v) = r.entry_value(at, ty, cnt).and_then(|b| {
                    if b.len() >= 4 {
                        Some(if little {
                            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
                        } else {
                            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
                        })
                    } else {
                        None
                    }
                }) {
                    exif_ifd = Some(v as usize);
                }
            }
            _ => {}
        }
        true
    });
    if let Some(off) = exif_ifd {
        r.walk_ifd(off, |tag, at, ty, cnt| {
            if tag == 0x9003 {
                // DateTimeOriginal supersedes the IFD0 DateTime.
                taken = r.ascii(at, ty, cnt).and_then(|s| parse_exif_datetime(&s));
            }
            true
        });
    }
    (make, model, taken)
}

/// "YYYY:MM:DD HH:MM:SS" → milliseconds since the Unix epoch (UTC).
fn parse_exif_datetime(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<i64> {
        let mut v: i64 = 0;
        for i in from..from + len {
            let c = *b.get(i)?;
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + i64::from(c - b'0');
        }
        Some(v)
    };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hh, mm, ss) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || year < 1970 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hh * 3_600 + mm * 60 + ss) * 1_000).max(0))
}

/// Howard Hinnant's days-from-civil: days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn jpeg_info(buf: &[u8]) -> Option<MediaInfo> {
    let mut info = empty();
    let mut at = 2usize;
    while at + 1 < buf.len() {
        // Markers are introduced by 0xFF (padding FFs allowed).
        if buf[at] != 0xFF {
            break;
        }
        let marker = buf[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            at += 2;
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            break; // EOI, or SOS: entropy data follows, headers are done.
        }
        let len = match buf.get(at + 2..at + 4) {
            Some(l) => u16::from_be_bytes([l[0], l[1]]) as usize,
            None => break,
        };
        if len < 2 {
            break;
        }
        let seg = buf.get(at + 4..at + 2 + len);
        match (marker, seg) {
            (0xE1, Some(seg)) if seg.starts_with(b"Exif\0\0") => {
                let (make, model, taken) = parse_exif(&seg[6..]);
                info.make = make;
                info.model = model;
                info.taken_at_ms = taken;
            }
            (0xC0, Some(seg)) | (0xC2, Some(seg)) => {
                // precision(1) height(2) width(2)
                if let (Some(h), Some(w)) = (
                    seg.get(1..3).map(|b| u16::from_be_bytes([b[0], b[1]])),
                    seg.get(3..5).map(|b| u16::from_be_bytes([b[0], b[1]])),
                ) {
                    info.height = Some(u32::from(h));
                    info.width = Some(u32::from(w));
                }
            }
            _ => {}
        }
        at += 2 + len;
    }
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_fixture(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v.extend_from_slice(&[0, 0, 0, 0]); // CRC (parser ignores)
        v
    }

    #[test]
    fn png_dimensions() {
        let info = media_info_from_bytes(&png_fixture(1920, 1080)).unwrap();
        assert_eq!(info.width, Some(1920));
        assert_eq!(info.height, Some(1080));
    }

    #[test]
    fn gif_dimensions() {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&320u16.to_le_bytes());
        v.extend_from_slice(&200u16.to_le_bytes());
        let info = media_info_from_bytes(&v).unwrap();
        assert_eq!((info.width, info.height), (Some(320), Some(200)));
    }

    #[test]
    fn bmp_dimensions_and_top_down() {
        let mut v = b"BM".to_vec();
        v.extend_from_slice(&[0; 12]);
        v.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
        v.extend_from_slice(&640i32.to_le_bytes());
        v.extend_from_slice(&(-480i32).to_le_bytes());
        let info = media_info_from_bytes(&v).unwrap();
        assert_eq!((info.width, info.height), (Some(640), Some(480)));
    }

    #[test]
    fn webp_vp8x_dimensions() {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8X");
        v.extend_from_slice(&10u32.to_le_bytes());
        v.extend_from_slice(&[0; 4]); // flags + reserved
        v.extend_from_slice(&799u32.to_le_bytes()[..3]); // width - 1
        v.extend_from_slice(&599u32.to_le_bytes()[..3]); // height - 1
        let info = media_info_from_bytes(&v).unwrap();
        assert_eq!((info.width, info.height), (Some(800), Some(600)));
    }

    #[test]
    fn webp_vp8l_dimensions() {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8L");
        v.extend_from_slice(&5u32.to_le_bytes());
        v.push(0x2F);
        let packed: u32 = (100 - 1) | ((50 - 1) << 14);
        v.extend_from_slice(&packed.to_le_bytes());
        let info = media_info_from_bytes(&v).unwrap();
        assert_eq!((info.width, info.height), (Some(100), Some(50)));
    }

    /// Minimal little-endian EXIF JPEG: APP1 with Make/Model in IFD0 and
    /// DateTimeOriginal in the EXIF IFD, then a SOF0 segment.
    fn jpeg_fixture() -> Vec<u8> {
        // TIFF layout (offsets relative to TIFF start):
        // 0: header(8)  8: IFD0  8+2+3*12+4 = 50: strings  EXIF IFD after.
        let mut t: Vec<u8> = Vec::new();
        t.extend_from_slice(b"II");
        t.extend_from_slice(&42u16.to_le_bytes());
        t.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
                                                  // IFD0: 3 entries (Make, Model, ExifIFD) + next(4)
        let ifd0 = 8usize;
        let strings_at = ifd0 + 2 + 3 * 12 + 4; // 50
        let make = b"Canon\0";
        let model = b"Canon EOS R5\0";
        let exif_ifd = strings_at + make.len() + model.len();
        let dto_at = exif_ifd + 2 + 12 + 4;
        let dto = b"2023:01:15 10:30:00\0";
        t.extend_from_slice(&3u16.to_le_bytes());
        // Make (tag 0x010F, ASCII, len 6 -> offset)
        t.extend_from_slice(&0x010Fu16.to_le_bytes());
        t.extend_from_slice(&2u16.to_le_bytes());
        t.extend_from_slice(&(make.len() as u32).to_le_bytes());
        t.extend_from_slice(&(strings_at as u32).to_le_bytes());
        // Model (0x0110)
        t.extend_from_slice(&0x0110u16.to_le_bytes());
        t.extend_from_slice(&2u16.to_le_bytes());
        t.extend_from_slice(&(model.len() as u32).to_le_bytes());
        t.extend_from_slice(&((strings_at + make.len()) as u32).to_le_bytes());
        // ExifIFD pointer (0x8769, LONG)
        t.extend_from_slice(&0x8769u16.to_le_bytes());
        t.extend_from_slice(&4u16.to_le_bytes());
        t.extend_from_slice(&1u32.to_le_bytes());
        t.extend_from_slice(&(exif_ifd as u32).to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        assert_eq!(t.len(), strings_at);
        t.extend_from_slice(make);
        t.extend_from_slice(model);
        assert_eq!(t.len(), exif_ifd);
        // EXIF IFD: 1 entry DateTimeOriginal (0x9003)
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&0x9003u16.to_le_bytes());
        t.extend_from_slice(&2u16.to_le_bytes());
        t.extend_from_slice(&(dto.len() as u32).to_le_bytes());
        t.extend_from_slice(&(dto_at as u32).to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(t.len(), dto_at);
        t.extend_from_slice(dto);

        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        let app1_len = (2 + 6 + t.len()) as u16;
        jpg.extend_from_slice(&app1_len.to_be_bytes());
        jpg.extend_from_slice(b"Exif\0\0");
        jpg.extend_from_slice(&t);
        // SOF0: height 480, width 640
        jpg.extend_from_slice(&[0xFF, 0xC0]);
        jpg.extend_from_slice(&17u16.to_be_bytes());
        jpg.push(8);
        jpg.extend_from_slice(&480u16.to_be_bytes());
        jpg.extend_from_slice(&640u16.to_be_bytes());
        jpg.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        jpg
    }

    #[test]
    fn jpeg_dimensions_and_exif() {
        let info = media_info_from_bytes(&jpeg_fixture()).unwrap();
        assert_eq!((info.width, info.height), (Some(640), Some(480)));
        assert_eq!(info.make.as_deref(), Some("Canon"));
        assert_eq!(info.model.as_deref(), Some("Canon EOS R5"));
        // 2023-01-15T10:30:00Z
        assert_eq!(info.taken_at_ms, Some(1_673_778_600_000));
    }

    #[test]
    fn truncated_and_garbage_never_panic() {
        let full = jpeg_fixture();
        for cut in [0, 1, 2, 5, 10, 20, 40, full.len() - 1] {
            let _ = media_info_from_bytes(&full[..cut.min(full.len())]);
        }
        let png = png_fixture(10, 10);
        for cut in [0, 4, 8, 12, 16, 20] {
            let _ = media_info_from_bytes(&png[..cut]);
        }
        assert!(media_info_from_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]).is_none());
        assert!(media_info_from_bytes(&[]).is_none());
        // A known signature with no parseable body is Some with no fields
        // (or None) — but never a panic; corrupt TIFF offsets included.
        let mut bad_exif = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x10];
        bad_exif.extend_from_slice(b"Exif\0\0II*\0\xFF\xFF\xFF\xFF");
        let _ = media_info_from_bytes(&bad_exif);
    }

    #[test]
    fn exif_datetime_epoch_math() {
        assert_eq!(parse_exif_datetime("1970:01:01 00:00:00"), Some(0));
        assert_eq!(
            parse_exif_datetime("2023:01:15 10:30:00"),
            Some(1_673_778_600_000)
        );
        assert_eq!(parse_exif_datetime("2023:13:01 00:00:00"), None);
        assert_eq!(parse_exif_datetime("garbage"), None);
    }
}
