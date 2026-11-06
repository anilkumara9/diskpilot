//! NTFS Master File Table parser — the pure half of the fast scan
//! path (the volume-reading half lives in `mft_volume`).
//!
//! Everything in this module works on byte slices: parse a boot
//! sector, parse individual file records (with Update Sequence Array
//! fixup validation), and assemble the surviving records into a flat
//! list of entries with full paths. No IO happens here, which is
//! what lets the test suite prove the parser on synthetic MFT images
//! on any platform.
//!
//! Format references are the public NTFS documentation: the boot
//! sector's BIOS parameter block, the file record layout, and the
//! $STANDARD_INFORMATION / $FILE_NAME / $DATA / $ATTRIBUTE_LIST
//! attribute layouts.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MftError {
    #[error("not an NTFS boot sector")]
    NotNtfs,
    #[error("invalid NTFS boot sector: {0}")]
    BadBootSector(&'static str),
    #[error("record buffer too small: {got} bytes for a {want}-byte record")]
    RecordTooSmall { got: usize, want: usize },
}

/// The geometry of an NTFS volume, from its boot sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSector {
    pub bytes_per_sector: u16,
    /// Cluster size in bytes (sectors-per-cluster already resolved,
    /// including the power-of-two encoding used for sub-sector
    /// clusters).
    pub cluster_bytes: u32,
    pub total_sectors: u64,
    /// LCN of the first cluster of the $MFT itself.
    pub mft_lcn: u64,
    /// File record size in bytes.
    pub record_size: u32,
}

/// Parse and validate an NTFS boot sector.
///
/// The two encoded-size fields follow the documented rules (the same
/// rules the in-kernel NTFS drivers implement): the clusters-per-
/// record byte, read as a signed value, means `2^|v|` bytes when
/// negative and `v × cluster size` when positive — the field is a
/// count of *clusters*, as its name says. The sectors-per-cluster
/// byte uses the same signed trick for clusters smaller than one
/// sector.
pub fn parse_boot_sector(bytes: &[u8]) -> Result<BootSector, MftError> {
    if bytes.len() < 96 {
        return Err(MftError::BadBootSector("shorter than the BPB fields"));
    }
    if &bytes[3..11] != b"NTFS    " {
        return Err(MftError::NotNtfs);
    }
    let bytes_per_sector = u16::from_le_bytes([bytes[11], bytes[12]]);
    if !bytes_per_sector.is_power_of_two() || !(256..=4096).contains(&bytes_per_sector) {
        return Err(MftError::BadBootSector("implausible bytes-per-sector"));
    }
    let spc_raw = bytes[13];
    if spc_raw == 0 {
        return Err(MftError::BadBootSector("zero sectors-per-cluster"));
    }
    let cluster_bytes = if spc_raw & 0x80 != 0 {
        let shift = (-(spc_raw as i8)) as u32;
        if shift > 24 {
            return Err(MftError::BadBootSector("implausible cluster size"));
        }
        1u32 << shift
    } else {
        spc_raw as u32 * bytes_per_sector as u32
    };
    let total_sectors = read_u64(bytes, 40);
    let mft_lcn = read_u64(bytes, 48);
    let rec_raw = bytes[64] as i8;
    let record_size = if rec_raw < 0 {
        let shift = (-(rec_raw as i32)) as u32;
        if shift > 24 {
            return Err(MftError::BadBootSector("implausible record size"));
        }
        1u32 << shift
    } else if rec_raw > 0 {
        rec_raw as u32 * cluster_bytes
    } else {
        return Err(MftError::BadBootSector("zero clusters-per-record"));
    };
    if record_size < bytes_per_sector as u32
        || record_size % bytes_per_sector as u32 != 0
        || record_size > (1 << 20)
    {
        return Err(MftError::BadBootSector("record size off the sector grid"));
    }
    Ok(BootSector {
        bytes_per_sector,
        cluster_bytes,
        total_sectors,
        mft_lcn,
        record_size,
    })
}

/// One data run from a runlist (data runs array).
///
/// `lcn_delta` is the signed delta against the previous run's LCN
/// (the first run's delta is its absolute LCN). A sparse run — a hole
/// with no on-disk allocation — is encoded on disk with no offset
/// bytes at all, which parses to a delta of exactly 0; a real run's
/// delta is never 0 in a valid runlist (consecutive runs cover
/// strictly advancing volume space), so `lcn_delta == 0` unambiguously
/// marks a sparse run here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataRun {
    pub lcn_delta: i64,
    pub length_clusters: u64,
}

/// Parse a runlist. Best-effort by design: parsing stops at the
/// terminator byte, at the end of the slice, or at a malformed run
/// header (a zero length-field size); the runs collected so far are
/// returned.
pub fn parse_runlist(bytes: &[u8]) -> Vec<DataRun> {
    let mut runs = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let header = bytes[pos];
        if header == 0 {
            break;
        }
        pos += 1;
        let len_size = (header & 0x0F) as usize;
        let ofs_size = (header >> 4) as usize;
        if len_size == 0 || len_size > 8 || ofs_size > 8 || pos + len_size + ofs_size > bytes.len()
        {
            break;
        }
        let mut length: u64 = 0;
        for i in 0..len_size {
            length |= (bytes[pos + i] as u64) << (8 * i);
        }
        pos += len_size;
        let mut delta: i64 = 0;
        if ofs_size > 0 {
            // Sign-extend from the field width.
            let mut raw: u64 = 0;
            for i in 0..ofs_size {
                raw |= (bytes[pos + i] as u64) << (8 * i);
            }
            let shift = 64 - 8 * ofs_size;
            delta = ((raw << shift) as i64) >> shift;
            pos += ofs_size;
        }
        runs.push(DataRun {
            lcn_delta: delta,
            length_clusters: length,
        });
    }
    runs
}

impl DataRun {
    pub fn is_sparse(&self) -> bool {
        self.lcn_delta == 0
    }
}

/// One $FILE_NAME attribute: a single name for a file in a single
/// parent directory. Several of these on one record are hard links
/// (plus, commonly, a short DOS alias of a long name).
#[derive(Debug, Clone, PartialEq)]
pub struct FileName {
    /// Low 48 bits of the parent directory's file reference.
    pub parent_ref: u64,
    /// 0 = POSIX, 1 = Win32, 2 = DOS, 3 = Win32 & DOS.
    pub namespace: u8,
    pub name: String,
    pub real_size: u64,
    /// The flags field of the $FILE_NAME itself (a copy of the
    /// file attributes, including the directory bit).
    pub flags: u32,
    pub modified_ms: i64,
}

/// The unnamed $DATA stream of one record (one instance — a
/// fragmented stream has one instance per extension record).
#[derive(Debug, Clone, PartialEq)]
pub struct DataInfo {
    pub lowest_vcn: u64,
    /// Real (logical) size of the stream. Every instance header of a
    /// fragmented stream repeats the stream's total real size.
    pub real_size: u64,
    /// Bytes this instance occupies on disk. For a non-resident
    /// instance this is `Σ non-sparse runs × cluster size` — the true
    /// allocation for uncompressed data (for compressed data it
    /// overstates what `GetCompressedFileSizeW` would report; a
    /// bounded divergence, accepted and documented). For a resident
    /// instance the content lives inside the file record itself, so
    /// the convention here is the volume's record size: a resident
    /// file's allocation is one record, which is what the space costs
    /// the volume.
    pub allocated_bytes: u64,
    pub resident: bool,
    pub runs: Vec<DataRun>,
}

/// One $ATTRIBUTE_LIST entry: an attribute instance that may live in
/// an extension record rather than in the base record.
#[derive(Debug, Clone, PartialEq)]
pub struct AttrListEntry {
    pub attr_type: u32,
    pub lowest_vcn: u64,
    /// Low 48 bits: the record holding this attribute instance.
    pub record_ref: u64,
    /// The attribute's own name (streams); empty for $FILE_NAME and
    /// for the unnamed $DATA.
    pub name: String,
}

/// A parsed file record, reduced to what assembly needs.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRecord {
    /// MFT record number. `parse_record` fills this from the
    /// record's own header field (offset 44, NTFS 3.1+); callers
    /// streaming records from a volume should overwrite it with the
    /// stream ordinal, which is authoritative.
    pub number: u64,
    pub in_use: bool,
    pub is_directory: bool,
    /// Low 48 bits of the base record reference; 0 for base records.
    pub base_ref: u64,
    /// File attributes (from $STANDARD_INFORMATION, falling back to
    /// the first $FILE_NAME's flags). Carries the reparse-point bit.
    pub file_attributes: u32,
    /// $STANDARD_INFORMATION modification time, when present.
    pub si_modified_ms: Option<i64>,
    pub names: Vec<FileName>,
    /// The unnamed $DATA instance living in THIS record, if any.
    pub data: Option<DataInfo>,
    pub attr_list: Vec<AttrListEntry>,
}

impl RawRecord {
    pub fn is_extension(&self) -> bool {
        self.base_ref != 0 && self.base_ref != self.number
    }
    pub fn is_reparse(&self) -> bool {
        self.file_attributes & 0x0400 != 0
    }
}

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

/// FILETIME (100 ns intervals since 1601-01-01) to Unix epoch
/// milliseconds; 0 for the zero time or anything before the epoch —
/// the same "no usable mtime" value the walker reports.
pub fn filetime_to_unix_ms(filetime: u64) -> i64 {
    if filetime == 0 {
        return 0;
    }
    let ms = (filetime / 10_000) as i64 - 11_644_473_600_000;
    ms.max(0)
}

/// Parse one file record.
///
/// Returns `Ok(None)` for a record that is not usable — bad magic, a
/// failed fixup (the on-disk corruption signature), or structure too
/// broken to trust — so the caller can skip and count it while its
/// neighbours survive. `Err` is reserved for a buffer that cannot
/// hold the record the boot sector describes.
pub fn parse_record(bytes: &[u8], boot: &BootSector) -> Result<Option<RawRecord>, MftError> {
    let record_size = boot.record_size as usize;
    if bytes.len() < record_size {
        return Err(MftError::RecordTooSmall {
            got: bytes.len(),
            want: record_size,
        });
    }
    let rec = &bytes[..record_size];
    if &rec[0..4] != b"FILE" {
        return Ok(None);
    }
    // Update Sequence Array fixup: the last word of every sector must
    // equal the sequence number, and is then replaced by the saved
    // original word. A mismatch means the record was torn or is
    // corrupt — skip it.
    let usa_ofs = read_u16(rec, 4) as usize;
    let usa_count = read_u16(rec, 6) as usize;
    let sector_size = boot.bytes_per_sector as usize;
    let expected_count = record_size / sector_size + 1;
    if usa_count != expected_count || usa_ofs + usa_count * 2 > record_size {
        return Ok(None);
    }
    let mut fixed = rec.to_vec();
    let usn = read_u16(&fixed, usa_ofs);
    for i in 0..usa_count - 1 {
        let tail = (i + 1) * sector_size - 2;
        if read_u16(&fixed, tail) != usn {
            return Ok(None);
        }
        let saved = read_u16(&fixed, usa_ofs + 2 + i * 2);
        fixed[tail] = saved as u8;
        fixed[tail + 1] = (saved >> 8) as u8;
    }

    let flags = read_u16(&fixed, 22);
    let mut record = RawRecord {
        number: read_u32(&fixed, 44) as u64,
        in_use: flags & 0x0001 != 0,
        is_directory: flags & 0x0002 != 0,
        base_ref: read_u64(&fixed, 32) & 0x0000_FFFF_FFFF_FFFF,
        file_attributes: 0,
        si_modified_ms: None,
        names: Vec::new(),
        data: None,
        attr_list: Vec::new(),
    };

    let mut pos = read_u16(&fixed, 20) as usize;
    loop {
        if pos + 16 > record_size {
            break;
        }
        let attr_type = read_u32(&fixed, pos);
        if attr_type == ATTR_END {
            break;
        }
        let attr_len = read_u32(&fixed, pos + 4) as usize;
        if attr_len < 16 || pos + attr_len > record_size {
            break;
        }
        let non_resident = fixed[pos + 8] != 0;
        let attr_name_len = fixed[pos + 9] as usize;
        let content = if non_resident {
            None
        } else {
            let value_len = read_u32(&fixed, pos + 16) as usize;
            let value_ofs = read_u16(&fixed, pos + 20) as usize;
            let start = pos + value_ofs;
            let end = start.saturating_add(value_len);
            if start <= pos + attr_len && end <= pos + attr_len {
                Some(&fixed[start..end])
            } else {
                None
            }
        };
        match attr_type {
            ATTR_STANDARD_INFORMATION if !non_resident => {
                if let Some(c) = content {
                    if c.len() >= 48 {
                        record.file_attributes = read_u32(c, 32);
                        record.si_modified_ms = Some(filetime_to_unix_ms(read_u64(c, 8)));
                    }
                }
            }
            ATTR_ATTRIBUTE_LIST if !non_resident => {
                if let Some(c) = content {
                    record.attr_list = parse_attr_list(c);
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                if let Some(c) = content {
                    if let Some(name) = parse_file_name(c) {
                        record.names.push(name);
                    }
                }
            }
            ATTR_DATA if attr_name_len == 0 && record.data.is_none() => {
                record.data = Some(parse_data(&fixed, pos, non_resident, boot));
            }
            _ => {}
        }
        pos += attr_len;
    }
    if record.file_attributes == 0 {
        if let Some(first) = record.names.first() {
            record.file_attributes = first.flags;
        }
    }
    Ok(Some(record))
}

fn parse_file_name(c: &[u8]) -> Option<FileName> {
    if c.len() < 66 {
        return None;
    }
    let name_len = c[64] as usize;
    let name_end = 66usize.saturating_add(name_len * 2);
    if name_end > c.len() {
        return None;
    }
    let units: Vec<u16> = c[66..name_end]
        .chunks_exact(2)
        .map(|ch| u16::from_le_bytes([ch[0], ch[1]]))
        .collect();
    Some(FileName {
        parent_ref: read_u64(c, 0) & 0x0000_FFFF_FFFF_FFFF,
        namespace: c[65],
        name: String::from_utf16_lossy(&units),
        real_size: read_u64(c, 48),
        flags: read_u32(c, 56),
        modified_ms: filetime_to_unix_ms(read_u64(c, 16)),
    })
}

fn parse_attr_list(c: &[u8]) -> Vec<AttrListEntry> {
    let mut entries = Vec::new();
    let mut pos = 0usize;
    while pos + 26 <= c.len() {
        let entry_len = read_u16(c, pos + 4) as usize;
        if entry_len < 26 || pos + entry_len > c.len() {
            break;
        }
        let name_len = c[pos + 6] as usize;
        let name_ofs = c[pos + 7] as usize;
        let name = if name_len > 0 && pos + name_ofs + name_len * 2 <= c.len() {
            let units: Vec<u16> = c[pos + name_ofs..pos + name_ofs + name_len * 2]
                .chunks_exact(2)
                .map(|ch| u16::from_le_bytes([ch[0], ch[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::new()
        };
        entries.push(AttrListEntry {
            attr_type: read_u32(c, pos),
            lowest_vcn: read_u64(c, pos + 8),
            record_ref: read_u64(c, pos + 16) & 0x0000_FFFF_FFFF_FFFF,
            name,
        });
        pos += entry_len;
    }
    entries
}

fn parse_data(rec: &[u8], pos: usize, non_resident: bool, boot: &BootSector) -> DataInfo {
    if !non_resident {
        let content_len = read_u32(rec, pos + 16) as u64;
        return DataInfo {
            lowest_vcn: 0,
            real_size: content_len,
            // Resident data lives inside the file record: the file's
            // allocation is the record itself (see DataInfo docs).
            allocated_bytes: boot.record_size as u64,
            resident: true,
            runs: Vec::new(),
        };
    }
    let run_ofs = read_u16(rec, pos + 32) as usize;
    let attr_len = read_u32(rec, pos + 4) as usize;
    let runs = if run_ofs < attr_len {
        parse_runlist(&rec[pos + run_ofs..pos + attr_len])
    } else {
        Vec::new()
    };
    let allocated = runs
        .iter()
        .filter(|r| !r.is_sparse())
        .map(|r| r.length_clusters)
        .sum::<u64>()
        .saturating_mul(boot.cluster_bytes as u64);
    DataInfo {
        lowest_vcn: read_u64(rec, pos + 16),
        real_size: read_u64(rec, pos + 48),
        allocated_bytes: allocated,
        resident: false,
        runs,
    }
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// One assembled file-system entry: a file or directory with its
/// full path. Paths are volume-root-relative with a leading `\`
/// (e.g. `\Windows\System32`); the volume layer anchors them at the
/// scanned root (`C:` + `\Windows\…` → `C:\Windows\…`).
#[derive(Debug, Clone, PartialEq)]
pub struct MftEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub modified_ms: i64,
}

/// Counters describing what assembly left out, and why. Nothing is
/// dropped silently — the volume layer can log these, and tests
/// assert on them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssemblyStats {
    /// Records not marked in-use.
    pub deleted_records: u64,
    /// NTFS metadata records (numbers 0–15), which directory
    /// enumeration never returns either, so the walker never sees
    /// them; skipped for equivalence.
    pub metadata_records: u64,
    /// Records carrying no usable $FILE_NAME.
    pub nameless_records: u64,
    /// Entries dropped because an ancestor record was missing,
    /// deleted, or part of a parent cycle.
    pub orphaned_entries: u64,
    /// Entries dropped because an ancestor is a reparse point (the
    /// walker never descends into one, so it never sees them).
    pub under_reparse_entries: u64,
    /// Files skipped because their $ATTRIBUTE_LIST named a $DATA
    /// instance that no supplied record contained.
    pub unresolved_data: u64,
    /// Reparse-point entries skipped per the walker's policy; their
    /// paths, for the summary's skipped list.
    pub skipped_reparse_paths: Vec<String>,
}

/// The record number of the root directory on every NTFS volume.
pub const ROOT_RECORD: u64 = 5;

/// Namespace preference when several $FILE_NAME attributes describe
/// one link: Win32 > Win32 & DOS > POSIX > DOS (the brief's order).
fn namespace_rank(namespace: u8) -> u8 {
    match namespace {
        1 => 0,
        3 => 1,
        0 => 2,
        2 => 3,
        _ => 4,
    }
}

/// The links a record contributes: one per surviving (parent, name)
/// after alias resolution.
///
/// Real NTFS records often carry both a long Win32 name and its
/// generated 8.3 DOS alias in the same parent. Emitting both would
/// double-count the file (the walker only ever enumerates the long
/// name), so within one parent a DOS name is dropped whenever the
/// record also has a non-DOS name there. A parent holding *only* a
/// DOS name keeps it — old volumes have files with nothing else.
fn record_links(merged_names: &[FileName]) -> Vec<(u64, FileName)> {
    // Best namespace per (parent, name).
    let mut best: Vec<(u64, FileName)> = Vec::new();
    for name in merged_names {
        if let Some(slot) = best
            .iter_mut()
            .find(|(p, n)| *p == name.parent_ref && n.name == name.name)
        {
            if namespace_rank(name.namespace) < namespace_rank(slot.1.namespace) {
                slot.1 = name.clone();
            }
        } else {
            best.push((name.parent_ref, name.clone()));
        }
    }
    // DOS-alias drop, per parent.
    let parents_with_real_names: std::collections::HashSet<u64> = best
        .iter()
        .filter(|(_, n)| n.namespace != 2)
        .map(|(p, _)| *p)
        .collect();
    best.retain(|(p, n)| n.namespace != 2 || !parents_with_real_names.contains(p));
    best
}

/// Index of everything assembly needs to look up by record number.
struct AssemblyIndex<'a> {
    by_number: std::collections::HashMap<u64, &'a RawRecord>,
    /// base record number → its extension record numbers.
    extensions_of: std::collections::HashMap<u64, Vec<u64>>,
}

impl<'a> AssemblyIndex<'a> {
    fn new(records: &'a [RawRecord]) -> Self {
        let mut by_number = std::collections::HashMap::new();
        let mut extensions_of: std::collections::HashMap<u64, Vec<u64>> =
            std::collections::HashMap::new();
        for record in records {
            by_number.entry(record.number).or_insert(record);
            if record.is_extension() {
                extensions_of
                    .entry(record.base_ref)
                    .or_default()
                    .push(record.number);
            }
        }
        Self {
            by_number,
            extensions_of,
        }
    }

    fn get(&self, number: u64) -> Option<&'a RawRecord> {
        self.by_number.get(&number).copied()
    }

    /// The records whose attribute instances merge into `base`:
    /// the base itself plus — when it carries an attribute list —
    /// exactly the records that list names; otherwise the records
    /// that declare `base` as their base (the common fragmented
    /// case is discoverable both ways).
    fn instance_records(&self, base: &'a RawRecord) -> Vec<&'a RawRecord> {
        let mut out: Vec<&RawRecord> = vec![base];
        if !base.attr_list.is_empty() {
            let mut seen: std::collections::HashSet<u64> = [base.number].into_iter().collect();
            for entry in &base.attr_list {
                if seen.insert(entry.record_ref) {
                    if let Some(rec) = self.get(entry.record_ref) {
                        if rec.number == base.number || rec.base_ref == base.number {
                            out.push(rec);
                        }
                    }
                }
            }
        } else if let Some(exts) = self.extensions_of.get(&base.number) {
            for &num in exts {
                if let Some(rec) = self.get(num) {
                    out.push(rec);
                }
            }
        }
        out
    }

    fn merged_names(&self, base: &RawRecord) -> Vec<FileName> {
        let mut names = Vec::new();
        for rec in self.instance_records(base) {
            names.extend(rec.names.iter().cloned());
        }
        names
    }

    /// Merged $DATA for a base record: total real size and summed
    /// allocation across all instances. `declared` reports whether
    /// an attribute list named a $DATA instance at all (its absence
    /// everywhere, when declared, is the unresolvable case).
    fn merged_data(&self, base: &RawRecord) -> (Option<DataInfo>, bool) {
        let declared = base
            .attr_list
            .iter()
            .any(|e| e.attr_type == ATTR_DATA && e.name.is_empty());
        let mut found: Option<DataInfo> = None;
        for rec in self.instance_records(base) {
            if let Some(data) = &rec.data {
                match &mut found {
                    None => found = Some(data.clone()),
                    Some(total) => {
                        total.real_size = total.real_size.max(data.real_size);
                        total.allocated_bytes =
                            total.allocated_bytes.saturating_add(data.allocated_bytes);
                        total.runs.extend(data.runs.iter().copied());
                        total.lowest_vcn = total.lowest_vcn.min(data.lowest_vcn);
                        total.resident = false;
                    }
                }
            }
        }
        (found, declared)
    }

    fn modified_ms(&self, base: &RawRecord) -> Option<i64> {
        self.instance_records(base)
            .iter()
            .filter_map(|rec| rec.si_modified_ms)
            .next()
    }

    fn is_reparse(&self, base: &RawRecord) -> bool {
        self.instance_records(base)
            .iter()
            .any(|rec| rec.is_reparse())
    }
}

/// Walk from `record`'s parent up to the root, returning the path
/// segments (leaf-first) or the reason the chain cannot be walked.
enum Chain {
    Segments(Vec<String>),
    Orphan,
    UnderReparse,
}

fn build_chain(index: &AssemblyIndex, parent_ref: u64, leaf_name: &str, root_number: u64) -> Chain {
    let mut segments = vec![leaf_name.to_string()];
    let mut visited: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut cur = parent_ref;
    loop {
        if cur == root_number {
            return Chain::Segments(segments);
        }
        if !visited.insert(cur) {
            return Chain::Orphan; // parent cycle
        }
        let Some(ancestor) = index.get(cur) else {
            return Chain::Orphan;
        };
        if !ancestor.in_use {
            return Chain::Orphan;
        }
        if index.is_reparse(ancestor) {
            return Chain::UnderReparse;
        }
        // The ancestor's own primary link: the best (parent, name)
        // among its merged names (directories have exactly one link
        // in practice; hard-linked directories cannot be created
        // through Win32).
        let names = index.merged_names(ancestor);
        let links = record_links(&names);
        let Some((next_parent, link)) = links.into_iter().min_by(|a, b| {
            namespace_rank(a.1.namespace)
                .cmp(&namespace_rank(b.1.namespace))
                .then(a.1.name.cmp(&b.1.name))
        }) else {
            return Chain::Orphan;
        };
        segments.push(link.name.clone());
        cur = next_parent;
    }
}

/// The unnamed-$DATA instances that merge into the record numbered
/// `base_number`, following its attribute list / extension records
/// exactly as assembly does. The volume layer uses this for record
/// 0: the $MFT's own extents are described by instances that live in
/// its extension records.
pub fn data_instances(records: &[RawRecord], base_number: u64) -> Vec<DataInfo> {
    let index = AssemblyIndex::new(records);
    let Some(base) = index.get(base_number) else {
        return Vec::new();
    };
    index
        .instance_records(base)
        .into_iter()
        .filter_map(|rec| rec.data.clone())
        .collect()
}

/// Assemble parsed records into the flat entry list the index
/// builder consumes. See [`assemble_with_stats`] for the counters.
pub fn assemble(records: &[RawRecord]) -> Vec<MftEntry> {
    assemble_with_stats(records).0
}

/// [`assemble`], plus the counters describing everything left out.
pub fn assemble_with_stats(records: &[RawRecord]) -> (Vec<MftEntry>, AssemblyStats) {
    let mut stats = AssemblyStats::default();
    let index = AssemblyIndex::new(records);
    // Root: record 5 by convention; fall back to a self-parented
    // record named "." (there is exactly one on a healthy volume).
    let root_number = if index.get(ROOT_RECORD).is_some() {
        ROOT_RECORD
    } else if let Some(root) = records.iter().find(|r| {
        r.names
            .iter()
            .any(|n| n.name == "." && n.parent_ref == r.number)
    }) {
        root.number
    } else {
        return (Vec::new(), stats);
    };

    let mut ordered: Vec<&RawRecord> = records.iter().collect();
    ordered.sort_by_key(|r| r.number);

    let mut entries: Vec<MftEntry> = Vec::new();
    for record in ordered {
        if record.number == root_number {
            continue;
        }
        if record.number < 16 {
            stats.metadata_records += 1;
            continue;
        }
        if record.is_extension() {
            continue; // merged into its base below
        }
        if !record.in_use {
            stats.deleted_records += 1;
            continue;
        }
        let names = index.merged_names(record);
        if names.is_empty() {
            stats.nameless_records += 1;
            continue;
        }
        let (data, declared) = index.merged_data(record);
        if declared && data.is_none() && !record.is_directory {
            stats.unresolved_data += 1;
            continue;
        }
        let reparse = index.is_reparse(record);
        let links = record_links(&names);
        for (parent_ref, link) in links {
            match build_chain(&index, parent_ref, &link.name, root_number) {
                Chain::Segments(segments) => {
                    let mut path = String::new();
                    for seg in segments.iter().rev() {
                        path.push('\\');
                        path.push_str(seg);
                    }
                    if reparse {
                        stats.skipped_reparse_paths.push(path);
                        continue;
                    }
                    let (logical, allocated) = if record.is_directory {
                        (0, 0)
                    } else {
                        match &data {
                            Some(d) => (d.real_size, d.allocated_bytes),
                            None => (0, 0),
                        }
                    };
                    entries.push(MftEntry {
                        path,
                        name: link.name.clone(),
                        is_dir: record.is_directory,
                        logical_bytes: logical,
                        allocated_bytes: allocated,
                        modified_ms: index.modified_ms(record).unwrap_or(link.modified_ms),
                    });
                }
                Chain::Orphan => stats.orphaned_entries += 1,
                Chain::UnderReparse => stats.under_reparse_entries += 1,
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    (entries, stats)
}

// ---------------------------------------------------------------------------
// Little-endian readers (bounds are the caller's contract; every
// call site above has already checked the enclosing structure).
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests;
