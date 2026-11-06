//! NTFS USN change journal records — the pure half of the live
//! index (the volume-reading half lives in `usn_volume`).
//!
//! Everything in this module works on byte slices: parse the
//! `USN_RECORD_V2` records a `FSCTL_READ_USN_JOURNAL` call returns
//! and classify each record's reason mask into the handful of
//! changes the live index acts on. No IO happens here, which is
//! what lets the test suite prove the parser on synthetic journal
//! buffers on any platform.
//!
//! Format reference is the public USN_RECORD_V2 documentation:
//! a variable-length record whose file name is stored as UTF-16LE
//! at a byte offset inside the record.

/// `USN_REASON_DATA_OVERWRITE`: file data was overwritten.
pub const REASON_DATA_OVERWRITE: u32 = 0x0000_0001;
/// `USN_REASON_DATA_EXTEND`: file data was appended to.
pub const REASON_DATA_EXTEND: u32 = 0x0000_0002;
/// `USN_REASON_DATA_TRUNCATION`: file data was truncated.
pub const REASON_DATA_TRUNCATION: u32 = 0x0000_0004;
/// `USN_REASON_NAMED_DATA_OVERWRITE`: a named stream was overwritten.
pub const REASON_NAMED_DATA_OVERWRITE: u32 = 0x0000_0010;
/// `USN_REASON_NAMED_DATA_EXTEND`: a named stream was extended.
pub const REASON_NAMED_DATA_EXTEND: u32 = 0x0000_0020;
/// `USN_REASON_NAMED_DATA_TRUNCATION`: a named stream was truncated.
pub const REASON_NAMED_DATA_TRUNCATION: u32 = 0x0000_0040;
/// `USN_REASON_FILE_CREATE`: the file or directory was created.
pub const REASON_FILE_CREATE: u32 = 0x0000_0100;
/// `USN_REASON_FILE_DELETE`: the file or directory was deleted.
pub const REASON_FILE_DELETE: u32 = 0x0000_0200;
/// `USN_REASON_RENAME_OLD_NAME`: this record carries a rename's
/// previous name.
pub const REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
/// `USN_REASON_RENAME_NEW_NAME`: this record carries a rename's
/// new name.
pub const REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;
/// `USN_REASON_BASIC_INFO_CHANGE`: timestamps or basic attributes
/// changed.
pub const REASON_BASIC_INFO_CHANGE: u32 = 0x0000_8000;
/// `USN_REASON_HARD_LINK_CHANGE`: a hard link was added or removed.
pub const REASON_HARD_LINK_CHANGE: u32 = 0x0001_0000;
/// `USN_REASON_REPARSE_POINT_CHANGE`: the reparse point changed.
pub const REASON_REPARSE_POINT_CHANGE: u32 = 0x0010_0000;
/// `USN_REASON_CLOSE`: set on the final record for a change, when
/// the reading application asked for close records. The live index
/// does not request them; the flag is never part of classification.
pub const REASON_CLOSE: u32 = 0x8000_0000;

/// The reason mask the live index reads with: every change kind it
/// classifies, nothing else (security/EA churn is not content).
pub const READ_REASON_MASK: u32 = REASON_DATA_OVERWRITE
    | REASON_DATA_EXTEND
    | REASON_DATA_TRUNCATION
    | REASON_NAMED_DATA_OVERWRITE
    | REASON_NAMED_DATA_EXTEND
    | REASON_NAMED_DATA_TRUNCATION
    | REASON_FILE_CREATE
    | REASON_FILE_DELETE
    | REASON_RENAME_OLD_NAME
    | REASON_RENAME_NEW_NAME
    | REASON_BASIC_INFO_CHANGE
    | REASON_HARD_LINK_CHANGE
    | REASON_REPARSE_POINT_CHANGE;

/// `FILE_ATTRIBUTE_DIRECTORY` in a record's file attributes.
pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

/// What a journal record means for the index, reduced from the raw
/// reason mask. A rename arrives as two records — one carrying
/// [`ChangeKind::RenamedFrom`] (the old parent + name), one
/// [`ChangeKind::RenamedTo`] (the new parent + name) — and the
/// applier treats them as a remove plus an upsert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Created,
    Deleted,
    Modified,
    RenamedFrom,
    RenamedTo,
}

/// One parsed `USN_RECORD_V2`.
#[derive(Debug, Clone, PartialEq)]
pub struct UsnRecord {
    /// Low 48 bits of the file reference number: the MFT record
    /// number of the changed file or directory. (The high 16 bits
    /// are a reuse sequence number, stripped here.)
    pub file_ref: u64,
    /// Low 48 bits of the parent directory's file reference.
    pub parent_ref: u64,
    /// The FULL 64-bit file reference, reuse sequence included.
    /// Two references with equal low 48 bits but different
    /// sequences are DIFFERENT files (the record number was
    /// recycled), so anything that caches per file — the path
    /// resolver's directory cache above all — keys by this, never
    /// by the stripped number.
    pub file_ref_full: u64,
    /// The FULL 64-bit parent reference, sequence included; the
    /// resolver keys its directory cache by this value.
    pub parent_ref_full: u64,
    /// The journal sequence number of this record.
    pub usn: i64,
    /// When the change was journaled, Unix epoch milliseconds
    /// (FILETIME converted; 0 for the zero time).
    pub timestamp_ms: i64,
    /// The raw reason mask, kept for diagnosis.
    pub reason: u32,
    /// The record's file attributes (carries the directory bit).
    pub file_attributes: u32,
    /// The file's name inside its parent (never a full path — path
    /// resolution walks the parent chain, see `usn_volume`).
    pub name: String,
}

impl UsnRecord {
    pub fn kind(&self) -> ChangeKind {
        classify_reason(self.reason)
    }

    pub fn is_directory(&self) -> bool {
        self.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    /// A hard link was added or removed for this file. The reason
    /// classifies as a plain modification, but for index accounting
    /// it is structural: which paths share this file's bytes
    /// changed, and no per-path delta can express that.
    pub fn is_hard_link_change(&self) -> bool {
        self.reason & REASON_HARD_LINK_CHANGE != 0
    }
}

/// Reduce a reason mask to the change the live index acts on.
///
/// Priority is deliberate: a delete beats everything (a file created
/// and deleted inside one poll window only needs the delete), a
/// rename's two halves beat a plain create (a moved file also picks
/// up create-adjacent bits), and anything left over — data writes,
/// basic-info changes, hard-link and reparse changes — is a
/// modification the applier resolves by re-statting the file.
pub fn classify_reason(reason: u32) -> ChangeKind {
    if reason & REASON_FILE_DELETE != 0 {
        ChangeKind::Deleted
    } else if reason & REASON_RENAME_OLD_NAME != 0 {
        ChangeKind::RenamedFrom
    } else if reason & REASON_RENAME_NEW_NAME != 0 {
        ChangeKind::RenamedTo
    } else if reason & REASON_FILE_CREATE != 0 {
        ChangeKind::Created
    } else {
        ChangeKind::Modified
    }
}

/// The fixed-size prefix of a USN_RECORD_V2, up to the file name.
const RECORD_HEADER_LEN: usize = 60;

/// Parse every complete `USN_RECORD_V2` in `buffer`.
///
/// The buffer is the record area of a `FSCTL_READ_USN_JOURNAL`
/// result (the caller strips the leading next-USN field). Parsing
/// stops at the first incomplete or implausible record — a
/// truncated tail record is the expected end of a chunked read, so
/// the good records before it are kept and the caller re-reads at
/// record granularity, never mid-record. A record whose declared
/// length is zero or shorter than the fixed header stops the walk
/// (trusting it would loop forever or read out of bounds).
pub fn parse_usn_records(buffer: &[u8]) -> Vec<UsnRecord> {
    let mut records = Vec::new();
    let mut pos = 0usize;
    while pos + RECORD_HEADER_LEN <= buffer.len() {
        let record_len = read_u32(buffer, pos) as usize;
        if record_len < RECORD_HEADER_LEN || pos + record_len > buffer.len() {
            break;
        }
        let rec = &buffer[pos..pos + record_len];
        // Major version 2 is the only layout this parser reads;
        // anything else (V3's 128-bit references, V4) ends the walk.
        if read_u16(rec, 4) != 2 {
            break;
        }
        let name_len = read_u16(rec, 56) as usize;
        let name_ofs = read_u16(rec, 58) as usize;
        let name = match rec.get(name_ofs..name_ofs.saturating_add(name_len)) {
            Some(bytes) if name_len % 2 == 0 => {
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|ch| u16::from_le_bytes([ch[0], ch[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
            }
            _ => break,
        };
        records.push(UsnRecord {
            file_ref: read_u64(rec, 8) & 0x0000_FFFF_FFFF_FFFF,
            parent_ref: read_u64(rec, 16) & 0x0000_FFFF_FFFF_FFFF,
            file_ref_full: read_u64(rec, 8),
            parent_ref_full: read_u64(rec, 16),
            usn: read_i64(rec, 24),
            timestamp_ms: crate::mft::filetime_to_unix_ms(read_u64(rec, 32)),
            reason: read_u32(rec, 40),
            file_attributes: read_u32(rec, 52),
            name,
        });
        pos += record_len;
    }
    records
}

fn read_u16(bytes: &[u8], ofs: usize) -> u16 {
    u16::from_le_bytes([bytes[ofs], bytes[ofs + 1]])
}

fn read_u32(bytes: &[u8], ofs: usize) -> u32 {
    u32::from_le_bytes([bytes[ofs], bytes[ofs + 1], bytes[ofs + 2], bytes[ofs + 3]])
}

fn read_u64(bytes: &[u8], ofs: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[ofs..ofs + 8]);
    u64::from_le_bytes(buf)
}

fn read_i64(bytes: &[u8], ofs: usize) -> i64 {
    read_u64(bytes, ofs) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one synthetic USN_RECORD_V2 buffer.
    fn record(
        file_ref: u64,
        parent_ref: u64,
        usn: i64,
        reason: u32,
        attrs: u32,
        name: &str,
    ) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let len = RECORD_HEADER_LEN + units.len() * 2;
        let mut buf = vec![0u8; len];
        buf[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        buf[4..6].copy_from_slice(&2u16.to_le_bytes());
        // File reference with a nonzero sequence number in the
        // high bits: the parser must strip it.
        buf[8..16].copy_from_slice(&(file_ref | (7u64 << 48)).to_le_bytes());
        buf[16..24].copy_from_slice(&parent_ref.to_le_bytes());
        buf[24..32].copy_from_slice(&usn.to_le_bytes());
        // 2024-01-01T00:00:00Z as FILETIME.
        buf[32..40].copy_from_slice(&133_485_408_000_000_000u64.to_le_bytes());
        buf[40..44].copy_from_slice(&reason.to_le_bytes());
        buf[52..56].copy_from_slice(&attrs.to_le_bytes());
        buf[56..58].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
        buf[58..60].copy_from_slice(&(RECORD_HEADER_LEN as u16).to_le_bytes());
        for (i, unit) in units.iter().enumerate() {
            buf[RECORD_HEADER_LEN + i * 2..RECORD_HEADER_LEN + i * 2 + 2]
                .copy_from_slice(&unit.to_le_bytes());
        }
        buf
    }

    #[test]
    fn parses_each_change_kind() {
        let cases = [
            (REASON_FILE_CREATE, ChangeKind::Created),
            (REASON_FILE_DELETE, ChangeKind::Deleted),
            (REASON_DATA_OVERWRITE, ChangeKind::Modified),
            (REASON_DATA_EXTEND | REASON_CLOSE, ChangeKind::Modified),
            (REASON_BASIC_INFO_CHANGE, ChangeKind::Modified),
            (REASON_RENAME_OLD_NAME, ChangeKind::RenamedFrom),
            (REASON_RENAME_NEW_NAME, ChangeKind::RenamedTo),
            // Create combined with a data write is still a create.
            (REASON_FILE_CREATE | REASON_DATA_EXTEND, ChangeKind::Created),
            // Created and deleted within one window: the delete wins.
            (REASON_FILE_CREATE | REASON_FILE_DELETE, ChangeKind::Deleted),
        ];
        for (i, (reason, want)) in cases.iter().enumerate() {
            let buf = record(100 + i as u64, 5, 1000, *reason, 0x20, "file.txt");
            let parsed = parse_usn_records(&buf);
            assert_eq!(parsed.len(), 1, "case {i}");
            assert_eq!(parsed[0].kind(), *want, "case {i}");
            assert_eq!(parsed[0].file_ref, 100 + i as u64, "sequence bits stripped");
            assert_eq!(parsed[0].parent_ref, 5);
            // The full file reference keeps the reuse sequence (the
            // fixture writes sequence 7 into its high bits; the
            // parent is written sequence-free, so it stays 5).
            assert_eq!(parsed[0].file_ref_full, (100 + i as u64) | (7u64 << 48));
            assert_eq!(parsed[0].parent_ref_full, 5);
            assert_eq!(parsed[0].usn, 1000);
            assert_eq!(parsed[0].name, "file.txt");
            assert_eq!(parsed[0].timestamp_ms, 1_704_067_200_000);
            assert!(!parsed[0].is_directory());
        }
    }

    #[test]
    fn parses_a_rename_pair_and_directory_flag() {
        let mut buf = record(200, 42, 2000, REASON_RENAME_OLD_NAME, 0x10, "old name");
        buf.extend_from_slice(&record(
            200,
            77,
            2008,
            REASON_RENAME_NEW_NAME,
            0x10,
            "new name",
        ));
        let parsed = parse_usn_records(&buf);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind(), ChangeKind::RenamedFrom);
        assert_eq!(parsed[0].name, "old name");
        assert_eq!(parsed[0].parent_ref, 42);
        assert!(parsed[0].is_directory());
        assert_eq!(parsed[1].kind(), ChangeKind::RenamedTo);
        assert_eq!(parsed[1].name, "new name");
        assert_eq!(parsed[1].parent_ref, 77);
    }

    #[test]
    fn truncated_tail_record_is_dropped_not_fatal() {
        let mut buf = record(300, 5, 3000, REASON_FILE_CREATE, 0x20, "kept.bin");
        let tail = record(301, 5, 3008, REASON_FILE_CREATE, 0x20, "lost.bin");
        buf.extend_from_slice(&tail[..tail.len() - 5]);
        let parsed = parse_usn_records(&buf);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "kept.bin");
    }

    #[test]
    fn empty_and_garbage_buffers_parse_to_nothing() {
        assert!(parse_usn_records(&[]).is_empty());
        assert!(parse_usn_records(&[0u8; 16]).is_empty());
        // A zero record length must stop the walk, not loop.
        assert!(parse_usn_records(&[0u8; 128]).is_empty());
        // A record claiming more than the buffer holds.
        let mut buf = record(1, 5, 8, REASON_FILE_CREATE, 0x20, "x");
        buf[0..4].copy_from_slice(&9999u32.to_le_bytes());
        assert!(parse_usn_records(&buf).is_empty());
    }

    #[test]
    fn read_mask_covers_every_classified_reason() {
        for bit in [
            REASON_DATA_OVERWRITE,
            REASON_DATA_EXTEND,
            REASON_DATA_TRUNCATION,
            REASON_FILE_CREATE,
            REASON_FILE_DELETE,
            REASON_RENAME_OLD_NAME,
            REASON_RENAME_NEW_NAME,
            REASON_BASIC_INFO_CHANGE,
        ] {
            assert_ne!(READ_REASON_MASK & bit, 0, "bit {bit:#x} missing");
        }
        // Close records are not requested.
        assert_eq!(READ_REASON_MASK & REASON_CLOSE, 0);
    }
}
