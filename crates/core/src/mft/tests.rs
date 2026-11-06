//! Fixture-driven tests for the MFT parser.
//!
//! The helper below builds synthetic NTFS images — a boot sector
//! and file records with correct Update Sequence Array fixups — so
//! the parser and the assembler are proven byte-for-byte on any
//! platform, without a Windows volume.

use super::*;

// ---------------------------------------------------------------------------
// Fixture construction
// ---------------------------------------------------------------------------

const SECTOR: u16 = 512;
const CLUSTER: u32 = 4096; // 8 sectors per cluster
const RECORD: u32 = 1024; // clusters-per-record byte 0xF6

fn boot() -> BootSector {
    parse_boot_sector(&boot_bytes(SECTOR, 8, -10)).unwrap()
}

fn boot_bytes(bytes_per_sector: u16, spc: u8, rec_byte: i8) -> Vec<u8> {
    boot_bytes_full(bytes_per_sector, spc, rec_byte, b"NTFS    ")
}

fn boot_bytes_full(bytes_per_sector: u16, spc: u8, rec_byte: i8, oem: &[u8; 8]) -> Vec<u8> {
    let mut b = vec![0u8; 512];
    b[0] = 0xEB;
    b[1] = 0x52;
    b[2] = 0x90;
    b[3..11].copy_from_slice(oem);
    b[11..13].copy_from_slice(&bytes_per_sector.to_le_bytes());
    b[13] = spc;
    b[40..48].copy_from_slice(&1_000_000u64.to_le_bytes()); // total sectors
    b[48..56].copy_from_slice(&4u64.to_le_bytes()); // $MFT at LCN 4
    b[64] = rec_byte as u8;
    b
}

/// 2020-01-01T00:00:00Z in FILETIME, and its Unix-ms equivalent.
const FT_2020: u64 = 132_223_104_000_000_000;
const MS_2020: i64 = 1_577_836_800_000;

struct RecordBuilder {
    buf: Vec<u8>,
    pos: usize,
    instance: u16,
}

impl RecordBuilder {
    fn new(number: u64, in_use: bool, is_dir: bool, base_ref: u64) -> Self {
        let record_size = RECORD as usize;
        let sector = SECTOR as usize;
        let usa_count = record_size / sector + 1;
        let usa_ofs = 0x30usize;
        let first_attr = usa_ofs + usa_count * 2;
        let first_attr = first_attr.div_ceil(8) * 8;
        let mut buf = vec![0u8; record_size];
        buf[0..4].copy_from_slice(b"FILE");
        buf[4..6].copy_from_slice(&(usa_ofs as u16).to_le_bytes());
        buf[6..8].copy_from_slice(&(usa_count as u16).to_le_bytes());
        buf[20..22].copy_from_slice(&(first_attr as u16).to_le_bytes());
        let mut flags: u16 = 0;
        if in_use {
            flags |= 0x0001;
        }
        if is_dir {
            flags |= 0x0002;
        }
        buf[22..24].copy_from_slice(&flags.to_le_bytes());
        buf[32..40].copy_from_slice(&base_ref.to_le_bytes());
        buf[44..48].copy_from_slice(&(number as u32).to_le_bytes());
        Self {
            buf,
            pos: first_attr,
            instance: 0,
        }
    }

    fn resident(mut self, attr_type: u32, content: &[u8]) -> Self {
        let start = self.pos;
        let total = 24 + content.len();
        let padded = total.div_ceil(8) * 8;
        self.buf[start..start + 4].copy_from_slice(&attr_type.to_le_bytes());
        self.buf[start + 4..start + 8].copy_from_slice(&(padded as u32).to_le_bytes());
        // non-resident flag (8) and name length (9) stay 0.
        self.buf[start + 14..start + 16].copy_from_slice(&self.instance.to_le_bytes());
        self.buf[start + 16..start + 20].copy_from_slice(&(content.len() as u32).to_le_bytes());
        self.buf[start + 20..start + 22].copy_from_slice(&24u16.to_le_bytes());
        self.buf[start + 24..start + 24 + content.len()].copy_from_slice(content);
        self.instance += 1;
        self.pos += padded;
        self
    }

    /// An unnamed non-resident $DATA instance.
    fn data_nonresident(mut self, lowest_vcn: u64, real_size: u64, runlist: &[u8]) -> Self {
        let start = self.pos;
        let total = 64 + runlist.len();
        let padded = total.div_ceil(8) * 8;
        self.buf[start..start + 4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        self.buf[start + 4..start + 8].copy_from_slice(&(padded as u32).to_le_bytes());
        self.buf[start + 8] = 1; // non-resident
        self.buf[start + 14..start + 16].copy_from_slice(&self.instance.to_le_bytes());
        self.buf[start + 16..start + 24].copy_from_slice(&lowest_vcn.to_le_bytes());
        self.buf[start + 32..start + 34].copy_from_slice(&64u16.to_le_bytes());
        self.buf[start + 48..start + 56].copy_from_slice(&real_size.to_le_bytes());
        self.buf[start + 64..start + 64 + runlist.len()].copy_from_slice(runlist);
        self.instance += 1;
        self.pos += padded;
        self
    }

    /// Write the end marker and apply the USA fixup with the given
    /// sequence number, exactly as NTFS writes a record out.
    fn finish(mut self, usn: u16) -> Vec<u8> {
        self.buf[self.pos..self.pos + 4].copy_from_slice(&ATTR_END.to_le_bytes());
        let record_size = self.buf.len();
        self.buf[24..28].copy_from_slice(&(self.pos as u32 + 8).to_le_bytes());
        let usa_ofs = read_u16(&self.buf, 4) as usize;
        let usa_count = read_u16(&self.buf, 6) as usize;
        let sector = SECTOR as usize;
        self.buf[usa_ofs..usa_ofs + 2].copy_from_slice(&usn.to_le_bytes());
        for i in 0..usa_count - 1 {
            let tail = (i + 1) * sector - 2;
            let original = read_u16(&self.buf, tail);
            self.buf[usa_ofs + 2 + i * 2..usa_ofs + 4 + i * 2]
                .copy_from_slice(&original.to_le_bytes());
            self.buf[tail..tail + 2].copy_from_slice(&usn.to_le_bytes());
        }
        debug_assert_eq!(record_size, RECORD as usize);
        self.buf
    }
}

fn si(modified_ft: u64, attrs: u32) -> Vec<u8> {
    let mut c = vec![0u8; 72];
    c[0..8].copy_from_slice(&modified_ft.to_le_bytes());
    c[8..16].copy_from_slice(&modified_ft.to_le_bytes());
    c[32..36].copy_from_slice(&attrs.to_le_bytes());
    c
}

fn file_name(parent: u64, namespace: u8, name: &str, real_size: u64) -> Vec<u8> {
    file_name_ft(parent, namespace, name, real_size, FT_2020)
}

fn file_name_ft(parent: u64, namespace: u8, name: &str, real_size: u64, ft: u64) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut c = vec![0u8; 66 + units.len() * 2];
    c[0..8].copy_from_slice(&parent.to_le_bytes());
    c[8..16].copy_from_slice(&ft.to_le_bytes());
    c[16..24].copy_from_slice(&ft.to_le_bytes());
    c[48..56].copy_from_slice(&real_size.to_le_bytes());
    c[64] = units.len() as u8;
    c[65] = namespace;
    for (i, u) in units.iter().enumerate() {
        c[66 + i * 2..68 + i * 2].copy_from_slice(&u.to_le_bytes());
    }
    c
}

fn attr_list(entries: &[(u32, u64, u64)]) -> Vec<u8> {
    let mut c = Vec::new();
    for &(attr_type, lowest_vcn, record_ref) in entries {
        let mut e = vec![0u8; 26];
        e[0..4].copy_from_slice(&attr_type.to_le_bytes());
        e[4..6].copy_from_slice(&26u16.to_le_bytes());
        e[8..16].copy_from_slice(&lowest_vcn.to_le_bytes());
        e[16..24].copy_from_slice(&record_ref.to_le_bytes());
        c.extend_from_slice(&e);
    }
    c
}

/// Encode one run for a runlist: (signed LCN delta, length in
/// clusters); `None` delta = sparse run.
fn run(delta: Option<i64>, length: u8) -> Vec<u8> {
    match delta {
        None => vec![0x01, length],
        Some(d) => {
            let b = d as u8;
            vec![0x11, length, b]
        }
    }
}

fn root_record() -> Vec<u8> {
    RecordBuilder::new(5, true, true, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 3, ".", 0))
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x06))
        .finish(7)
}

fn parse(bytes: &[u8]) -> RawRecord {
    parse_record(bytes, &boot())
        .expect("record parses")
        .expect("record usable")
}

// ---------------------------------------------------------------------------
// Boot sector
// ---------------------------------------------------------------------------

#[test]
fn boot_negative_record_size_branch() {
    let boot = boot();
    assert_eq!(boot.bytes_per_sector, 512);
    assert_eq!(boot.cluster_bytes, CLUSTER);
    assert_eq!(boot.record_size, 1024); // 0xF6 = -10 → 2^10
    assert_eq!(boot.mft_lcn, 4);
}

#[test]
fn boot_positive_record_size_branch_is_clusters() {
    // Positive byte: record size = value × cluster size.
    let one = parse_boot_sector(&boot_bytes(512, 8, 1)).unwrap();
    assert_eq!(one.record_size, CLUSTER);
    let two = parse_boot_sector(&boot_bytes(512, 8, 2)).unwrap();
    assert_eq!(two.record_size, 2 * CLUSTER);
}

#[test]
fn boot_rejects_non_ntfs() {
    let bytes = boot_bytes_full(512, 8, 0xF6u8 as i8, b"FAT32   ");
    assert!(matches!(parse_boot_sector(&bytes), Err(MftError::NotNtfs)));
    assert!(matches!(
        parse_boot_sector(&[0u8; 32]),
        Err(MftError::BadBootSector(_))
    ));
}

// ---------------------------------------------------------------------------
// Runlists
// ---------------------------------------------------------------------------

#[test]
fn runlist_deltas_sparse_and_terminator() {
    let mut bytes = Vec::new();
    bytes.extend(run(Some(100), 4));
    bytes.extend(run(None, 2)); // sparse
    bytes.extend(run(Some(-30), 3));
    bytes.push(0); // terminator
    bytes.extend(run(Some(9), 9)); // garbage after the terminator
    let runs = parse_runlist(&bytes);
    assert_eq!(
        runs,
        vec![
            DataRun {
                lcn_delta: 100,
                length_clusters: 4
            },
            DataRun {
                lcn_delta: 0,
                length_clusters: 2
            },
            DataRun {
                lcn_delta: -30,
                length_clusters: 3
            },
        ]
    );
    assert!(!runs[0].is_sparse());
    assert!(runs[1].is_sparse());
    assert_eq!(parse_runlist(&[]), Vec::new());
}

// ---------------------------------------------------------------------------
// Records & assembly: one small tree, asserted exactly
// ---------------------------------------------------------------------------

fn small_tree_records() -> Vec<RawRecord> {
    let root = root_record();
    let dir = RecordBuilder::new(16, true, true, 0)
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x06))
        .resident(ATTR_FILE_NAME, &file_name(5, 3, "dir", 0))
        .finish(7);
    let a_txt = RecordBuilder::new(17, true, false, 0)
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x20))
        .resident(ATTR_FILE_NAME, &file_name(16, 3, "a.txt", 100))
        .resident(ATTR_DATA, &[7u8; 100])
        .finish(7);
    let b_bin = RecordBuilder::new(18, true, false, 0)
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x20))
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "b.bin", 0))
        .resident(ATTR_DATA, &[])
        .finish(7);
    vec![parse(&root), parse(&dir), parse(&a_txt), parse(&b_bin)]
}

#[test]
fn small_tree_assembles_exactly() {
    let entries = assemble(&small_tree_records());
    assert_eq!(
        entries,
        vec![
            MftEntry {
                path: "\\b.bin".into(),
                name: "b.bin".into(),
                is_dir: false,
                logical_bytes: 0,
                // Resident $DATA: allocation is one MFT record.
                allocated_bytes: RECORD as u64,
                modified_ms: MS_2020,
            },
            MftEntry {
                path: "\\dir".into(),
                name: "dir".into(),
                is_dir: true,
                logical_bytes: 0,
                allocated_bytes: 0,
                modified_ms: MS_2020,
            },
            MftEntry {
                path: "\\dir\\a.txt".into(),
                name: "a.txt".into(),
                is_dir: false,
                logical_bytes: 100,
                allocated_bytes: RECORD as u64,
                modified_ms: MS_2020,
            },
        ]
    );
}

// ---------------------------------------------------------------------------
// Non-resident allocation
// ---------------------------------------------------------------------------

#[test]
fn nonresident_allocated_is_runs_times_cluster() {
    let mut runlist = Vec::new();
    runlist.extend(run(Some(10), 2));
    runlist.extend(run(Some(5), 1));
    runlist.push(0);
    let file = RecordBuilder::new(19, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "big.dat", 10_000))
        .data_nonresident(0, 10_000, &runlist)
        .finish(7);
    let entries = assemble(&[parse(&root_record()), parse(&file)]);
    let big = entries.iter().find(|e| e.name == "big.dat").unwrap();
    assert_eq!(big.logical_bytes, 10_000);
    assert_eq!(big.allocated_bytes, 3 * CLUSTER as u64);
}

// ---------------------------------------------------------------------------
// Deleted records, DOS names, aliases, hard links
// ---------------------------------------------------------------------------

#[test]
fn deleted_record_is_skipped() {
    let ghost = RecordBuilder::new(20, false, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "gone.txt", 1))
        .resident(ATTR_DATA, &[1])
        .finish(7);
    let (entries, stats) = assemble_with_stats(&[parse(&root_record()), parse(&ghost)]);
    assert!(entries.is_empty());
    assert_eq!(stats.deleted_records, 1);
}

#[test]
fn dos_only_name_is_kept() {
    let old = RecordBuilder::new(20, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 2, "OLD.TXT", 3))
        .resident(ATTR_DATA, &[1, 2, 3])
        .finish(7);
    let entries = assemble(&[parse(&root_record()), parse(&old)]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "\\OLD.TXT");
}

#[test]
fn dos_alias_does_not_double_count() {
    let file = RecordBuilder::new(21, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "long name.txt", 5))
        .resident(ATTR_FILE_NAME, &file_name(5, 2, "LONGNA~1.TXT", 5))
        .resident(ATTR_DATA, &[0u8; 5])
        .finish(7);
    let entries = assemble(&[parse(&root_record()), parse(&file)]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "long name.txt");
}

#[test]
fn hardlink_yields_two_entries() {
    let dir = RecordBuilder::new(16, true, true, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 3, "dir", 0))
        .finish(7);
    let file = RecordBuilder::new(22, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "one.txt", 4))
        .resident(ATTR_FILE_NAME, &file_name(16, 1, "two.txt", 4))
        .resident(ATTR_DATA, &[0u8; 4])
        .finish(7);
    let entries = assemble(&[parse(&root_record()), parse(&dir), parse(&file)]);
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.name.as_str())
        .collect();
    // Entries come out sorted by full path, so the dir link
    // (`\dir\two.txt`) precedes the root link (`\one.txt`).
    assert_eq!(names, vec!["two.txt", "one.txt"]);
    assert!(entries
        .iter()
        .filter(|e| !e.is_dir)
        .all(|e| e.allocated_bytes == RECORD as u64));
}

// ---------------------------------------------------------------------------
// Fragmented $DATA through an attribute list + extension record
// ---------------------------------------------------------------------------

#[test]
fn fragmented_data_merges_across_extension_record() {
    let mut runs_a = Vec::new();
    runs_a.extend(run(Some(20), 2));
    runs_a.push(0);
    let mut runs_b = Vec::new();
    runs_b.extend(run(Some(9), 1));
    runs_b.push(0);
    let base = RecordBuilder::new(23, true, false, 0)
        .resident(
            ATTR_ATTRIBUTE_LIST,
            &attr_list(&[(ATTR_DATA, 0, 23), (ATTR_DATA, 2, 24)]),
        )
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "frag.bin", 20_000))
        .data_nonresident(0, 20_000, &runs_a)
        .finish(7);
    let extension = RecordBuilder::new(24, true, false, 23)
        .data_nonresident(2, 20_000, &runs_b)
        .finish(7);
    let entries = assemble(&[parse(&root_record()), parse(&base), parse(&extension)]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].logical_bytes, 20_000);
    assert_eq!(entries[0].allocated_bytes, 3 * CLUSTER as u64);
}

// ---------------------------------------------------------------------------
// Corruption: one torn record, its neighbour survives
// ---------------------------------------------------------------------------

#[test]
fn corrupt_usa_record_is_skipped() {
    let mut torn = RecordBuilder::new(25, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "torn.txt", 1))
        .resident(ATTR_DATA, &[9])
        .finish(7);
    // Corrupt the fixup: the last word of the first sector no longer
    // matches the update sequence number.
    let tail = SECTOR as usize - 2;
    torn[tail] ^= 0xFF;
    assert!(parse_record(&torn, &boot()).unwrap().is_none());
    let good = RecordBuilder::new(26, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "good.txt", 1))
        .resident(ATTR_DATA, &[9])
        .finish(7);
    assert!(parse_record(&good, &boot()).unwrap().is_some());
}

#[test]
fn record_buffer_too_small_is_an_error() {
    let short = vec![0u8; 100];
    assert!(matches!(
        parse_record(&short, &boot()),
        Err(MftError::RecordTooSmall { .. })
    ));
}

// ---------------------------------------------------------------------------
// Orphans and reparse points
// ---------------------------------------------------------------------------

#[test]
fn orphan_parent_is_skipped_and_counted() {
    let stray = RecordBuilder::new(26, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(999, 1, "stray.txt", 1))
        .resident(ATTR_DATA, &[1])
        .finish(7);
    let (entries, stats) = assemble_with_stats(&[parse(&root_record()), parse(&stray)]);
    assert!(entries.is_empty());
    assert_eq!(stats.orphaned_entries, 1);
}

#[test]
fn parent_cycle_is_an_orphan_not_a_hang() {
    let a = RecordBuilder::new(30, true, true, 0)
        .resident(ATTR_FILE_NAME, &file_name(31, 3, "a", 0))
        .finish(7);
    let b = RecordBuilder::new(31, true, true, 0)
        .resident(ATTR_FILE_NAME, &file_name(30, 3, "b", 0))
        .finish(7);
    let file = RecordBuilder::new(32, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(30, 1, "x.txt", 1))
        .resident(ATTR_DATA, &[1])
        .finish(7);
    let (entries, stats) =
        assemble_with_stats(&[parse(&root_record()), parse(&a), parse(&b), parse(&file)]);
    assert!(entries.is_empty());
    assert!(stats.orphaned_entries >= 1);
}

#[test]
fn reparse_directory_and_children_are_skipped() {
    let link_dir = RecordBuilder::new(27, true, true, 0)
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x0400))
        .resident(ATTR_FILE_NAME, &file_name(5, 3, "linkdir", 0))
        .finish(7);
    let child = RecordBuilder::new(28, true, false, 0)
        .resident(ATTR_FILE_NAME, &file_name(27, 1, "child.txt", 1))
        .resident(ATTR_DATA, &[1])
        .finish(7);
    let (entries, stats) =
        assemble_with_stats(&[parse(&root_record()), parse(&link_dir), parse(&child)]);
    assert!(entries.is_empty());
    assert_eq!(stats.skipped_reparse_paths, vec!["\\linkdir".to_string()]);
    assert_eq!(stats.under_reparse_entries, 1);
}

#[test]
fn reparse_file_is_skipped() {
    let link = RecordBuilder::new(29, true, false, 0)
        .resident(ATTR_STANDARD_INFORMATION, &si(FT_2020, 0x0400))
        .resident(ATTR_FILE_NAME, &file_name(5, 1, "link.txt", 1))
        .resident(ATTR_DATA, &[1])
        .finish(7);
    let (entries, stats) = assemble_with_stats(&[parse(&root_record()), parse(&link)]);
    assert!(entries.is_empty());
    assert_eq!(stats.skipped_reparse_paths, vec!["\\link.txt".to_string()]);
}

// ---------------------------------------------------------------------------
// Timestamp conversion
// ---------------------------------------------------------------------------

#[test]
fn filetime_converts() {
    assert_eq!(filetime_to_unix_ms(FT_2020), MS_2020);
    assert_eq!(filetime_to_unix_ms(0), 0);
    assert_eq!(filetime_to_unix_ms(1), 0); // before the epoch → 0
}
