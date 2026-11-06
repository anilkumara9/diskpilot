//! Volume IO for the fast scan path: reading the Master File Table
//! straight off an NTFS volume.
//!
//! The parser (`mft`) is pure; this module is the thin, Windows-only
//! shell around it. It opens the raw volume (`\\.\C:`), which the OS
//! only allows for administrators — when the process lacks access,
//! or the volume is not NTFS, or anything at all goes wrong, the
//! caller gets a typed [`MftUnavailable`] and falls back to the
//! standard filesystem walk. There is deliberately no elevation
//! prompt and no settings toggle: the fast path engages when the
//! process already has access and is invisible otherwise.
//!
//! On non-Windows platforms [`scan_volume`] always returns
//! [`MftUnavailable::Unsupported`].

use crate::mft::MftEntry;
use std::path::{Component, Path, Prefix};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MftUnavailable {
    #[error("the fast scan path is only available on Windows")]
    Unsupported,
    #[error("the scan root is not a volume root")]
    NotVolumeRoot,
    #[error("could not open the volume (administrator rights are required): {0}")]
    OpenVolume(String),
    #[error("the volume is not NTFS")]
    NotNtfs,
    #[error("could not read the volume: {0}")]
    ReadFailed(String),
    #[error("the fast path produced no files")]
    Empty,
}

/// A completed fast-path volume read.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeScan {
    /// All assembled entries, anchored at the canonical volume
    /// prefix (e.g. `C:\Windows\…`).
    pub entries: Vec<MftEntry>,
    /// Paths of reparse-point entries the assembly skipped, in the
    /// same anchored form, for the summary's skipped list.
    pub skipped: Vec<String>,
}

/// The canonical prefix of a volume root: the drive prefix with a
/// trailing backslash (`C:\`). `None` for anything that is not a
/// plain drive-letter root (UNC shares, relative paths…).
pub fn volume_prefix(root: &Path) -> Option<String> {
    let mut components = root.components();
    let prefix = match components.next()? {
        Component::Prefix(p) => p,
        _ => return None,
    };
    match prefix.kind() {
        Prefix::Disk(_) | Prefix::VerbatimDisk(_) => {
            let raw = prefix.as_os_str().to_string_lossy();
            let trimmed = raw.trim_end_matches(['\\', '/']);
            Some(format!("{trimmed}\\"))
        }
        _ => None,
    }
}

/// Is `path` a whole-volume root (`C:\` and nothing more)? The fast
/// path only ever engages for those: the MFT describes an entire
/// volume, so a sub-tree root could not use it without reading (and
/// trusting) far more than the user asked to scan.
pub fn is_volume_root(path: &Path) -> bool {
    if volume_prefix(path).is_none() {
        return false;
    }
    let mut components = path.components();
    matches!(components.next(), Some(Component::Prefix(_)))
        && matches!(components.next(), Some(Component::RootDir))
        && components.next().is_none()
}

/// A reusable reader over one NTFS volume's Master File Table,
/// for callers that need individual records rather than a whole
/// scan — the USN live index resolves changed files' paths by
/// reading their parent directories' records through this.
///
/// Opening performs the same bootstrap as a volume scan (boot
/// sector, record 0, the $MFT run map) and fails with the same
/// typed [`MftUnavailable`] when the volume cannot be read, so the
/// caller quietly falls back exactly as the scan path does.
pub struct MftReader {
    #[cfg(windows)]
    inner: imp::VolumeReader,
    #[cfg(not(windows))]
    _unsupported: std::marker::PhantomData<()>,
}

/// Open an [`MftReader`] for the volume `root` lives on. `root`
/// may be any path with a drive prefix (the volume device is
/// derived from the prefix alone); a volume root is the natural
/// call.
pub fn open_reader(root: &Path) -> Result<MftReader, MftUnavailable> {
    #[cfg(windows)]
    {
        Ok(MftReader {
            inner: imp::VolumeReader::open(root)?,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Err(MftUnavailable::Unsupported)
    }
}

impl MftReader {
    /// Read and parse the MFT record with the given number (the
    /// low 48 bits of a file reference). `None` when the record
    /// cannot be read, is not in use in a form the parser accepts,
    /// or simply does not parse — resolution callers treat that as
    /// "path unknown" and move on.
    pub fn read_record(&mut self, record_number: u64) -> Option<crate::mft::RawRecord> {
        #[cfg(windows)]
        {
            self.inner.read_record(record_number)
        }
        #[cfg(not(windows))]
        {
            let _ = record_number;
            None
        }
    }
}

/// Read the whole volume rooted at `root` through its MFT.
pub fn scan_volume(root: &Path) -> Result<VolumeScan, MftUnavailable> {
    if !is_volume_root(root) {
        return Err(MftUnavailable::NotVolumeRoot);
    }
    #[cfg(windows)]
    {
        imp::scan_volume(root)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Err(MftUnavailable::Unsupported)
    }
}

/// Anchor root-relative assembled paths (`\Windows\…`) and skipped
/// paths at `prefix` (the canonical `C:\` form).
#[cfg(windows)]
fn anchor(entries: Vec<MftEntry>, skipped: Vec<String>, prefix: &str) -> VolumeScan {
    let base = prefix.trim_end_matches('\\');
    let entries = entries
        .into_iter()
        .map(|mut e| {
            e.path = format!("{base}{}", e.path);
            e
        })
        .collect();
    let skipped = skipped.into_iter().map(|p| format!("{base}{p}")).collect();
    VolumeScan { entries, skipped }
}

#[cfg(windows)]
mod imp {
    use super::{anchor, volume_prefix, MftUnavailable, VolumeScan};
    use crate::mft::{self, BootSector, RawRecord};
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, SetFilePointerEx, FILE_BEGIN, FILE_FLAGS_AND_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    /// Bytes read from the volume per ReadFile call. A multiple of
    /// every plausible record size, so records never straddle reads.
    const CHUNK: usize = 8 * 1024 * 1024;

    struct Volume(HANDLE);

    impl Drop for Volume {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    impl Volume {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), MftUnavailable> {
            unsafe {
                SetFilePointerEx(self.0, offset as i64, None, FILE_BEGIN)
                    .map_err(|e| MftUnavailable::ReadFailed(format!("seek to {offset}: {e}")))?;
            }
            let mut filled = 0usize;
            while filled < buf.len() {
                let mut read: u32 = 0;
                unsafe {
                    ReadFile(self.0, Some(&mut buf[filled..]), Some(&mut read), None).map_err(
                        |e| MftUnavailable::ReadFailed(format!("read at {offset}: {e}")),
                    )?;
                }
                if read == 0 {
                    return Err(MftUnavailable::ReadFailed(format!(
                        "short read at {offset}"
                    )));
                }
                filled += read as usize;
            }
            Ok(())
        }
    }

    fn open_volume(root: &Path) -> Result<Volume, MftUnavailable> {
        let prefix = volume_prefix(root).ok_or(MftUnavailable::NotVolumeRoot)?;
        // "\\.\C:" — the raw volume device path for drive C.
        let device = format!("\\\\.\\{}", prefix.trim_end_matches('\\'));
        let wide: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(|e| MftUnavailable::OpenVolume(format!("{device}: {e}")))?;
        Ok(Volume(handle))
    }

    /// VCN → volume byte offset map for one file's data runs.
    struct RunMap {
        /// (starting VCN, starting volume offset, length in clusters);
        /// sparse segments carry a `None` offset.
        segments: Vec<(u64, Option<u64>, u64)>,
        cluster_bytes: u64,
    }

    impl RunMap {
        fn from_instances(instances: &[crate::mft::DataInfo], cluster_bytes: u64) -> RunMap {
            let mut sorted: Vec<&crate::mft::DataInfo> = instances.iter().collect();
            sorted.sort_by_key(|d| d.lowest_vcn);
            let mut segments = Vec::new();
            let mut prev_lcn: i64 = 0;
            for info in sorted {
                let mut vcn = info.lowest_vcn;
                for run in &info.runs {
                    if run.is_sparse() {
                        segments.push((vcn, None, run.length_clusters));
                    } else {
                        prev_lcn += run.lcn_delta;
                        segments.push((
                            vcn,
                            Some(prev_lcn as u64 * cluster_bytes),
                            run.length_clusters,
                        ));
                    }
                    vcn += run.length_clusters;
                }
            }
            RunMap {
                segments,
                cluster_bytes,
            }
        }

        fn total_bytes(&self) -> u64 {
            self.segments
                .iter()
                .map(|(_, _, len)| len * self.cluster_bytes)
                .sum()
        }

        /// Volume offset for a file byte offset; `None` inside a
        /// sparse segment.
        fn translate(&self, file_offset: u64) -> Option<u64> {
            let cluster = self.cluster_bytes;
            let vcn = file_offset / cluster;
            let within = file_offset % cluster;
            for &(start, offset, len) in &self.segments {
                if vcn >= start && vcn < start + len {
                    let base = offset?;
                    return Some(base + (vcn - start) * cluster + within);
                }
            }
            None
        }
    }

    fn read_one_record(
        volume: &Volume,
        map: &RunMap,
        boot: &BootSector,
        ordinal: u64,
        scratch: &mut [u8],
    ) -> Result<Option<RawRecord>, MftUnavailable> {
        let record_size = boot.record_size as u64;
        let file_offset = ordinal * record_size;
        let Some(mut volume_offset) = map.translate(file_offset) else {
            return Ok(None);
        };
        // Records never straddle run boundaries in practice (runs are
        // cluster-aligned and records divide clusters), but read
        // sector-by-sector through the map if one ever would.
        let mut filled = 0usize;
        while filled < scratch.len() {
            let step = (scratch.len() - filled).min(boot.bytes_per_sector as usize);
            volume.read_at(volume_offset, &mut scratch[filled..filled + step])?;
            filled += step;
            if filled < scratch.len() {
                match map.translate(file_offset + filled as u64) {
                    Some(next) => volume_offset = next,
                    None => return Ok(None),
                }
            }
        }
        match mft::parse_record(scratch, boot) {
            Ok(Some(mut record)) => {
                record.number = ordinal;
                Ok(Some(record))
            }
            Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    /// An opened volume with the $MFT bootstrap completed: the
    /// boot sector parsed and the run map spanning the whole $MFT,
    /// so any individual record can be read on demand. The public
    /// [`super::MftReader`] wraps this for single-record callers
    /// (the live index's path resolution); [`scan_volume`] streams
    /// the whole table through it.
    pub(super) struct VolumeReader {
        volume: Volume,
        boot: BootSector,
        map: RunMap,
        mft_real_size: u64,
        /// Records read during the bootstrap (record 0 is always
        /// among them), kept for the streaming caller.
        bootstrap_records: Vec<RawRecord>,
        /// Next record ordinal the bootstrap did not read.
        parsed_up_to: u64,
    }

    impl VolumeReader {
        pub(super) fn open(root: &Path) -> Result<VolumeReader, MftUnavailable> {
            let volume = open_volume(root)?;

            // Boot sector.
            let mut boot_bytes = vec![0u8; 512];
            volume.read_at(0, &mut boot_bytes)?;
            let boot = mft::parse_boot_sector(&boot_bytes).map_err(|e| match e {
                mft::MftError::NotNtfs => MftUnavailable::NotNtfs,
                other => MftUnavailable::ReadFailed(other.to_string()),
            })?;
            let record_size = boot.record_size as usize;

            // Record 0 ($MFT itself) sits at the MFT's first LCN.
            let mut scratch = vec![0u8; record_size];
            volume.read_at(boot.mft_lcn * boot.cluster_bytes as u64, &mut scratch)?;
            let mft_record = mft::parse_record(&scratch, &boot)
                .ok()
                .flatten()
                .ok_or_else(|| MftUnavailable::ReadFailed("record 0 ($MFT) did not parse".into()))?;
            let mft_real_size = mft_record
                .data
                .as_ref()
                .map(|d| d.real_size)
                .ok_or_else(|| MftUnavailable::ReadFailed("$MFT has no $DATA".into()))?;
            if mft_real_size == 0 || mft_real_size % record_size as u64 != 0 {
                return Err(MftUnavailable::ReadFailed("implausible $MFT size".into()));
            }

            // Bootstrap the run map for the whole $MFT: its later $DATA
            // instances live in extension records that are themselves
            // inside the MFT, so grow the map — read what the current map
            // covers, re-merge record 0's instances, repeat — until it
            // spans the file or stops growing.
            let mut records: Vec<RawRecord> = Vec::new();
            let mut parsed_up_to: u64 = 0; // next record ordinal to read
            let map = loop {
                let instances = mft::data_instances(&records_with_mft(&records, &mft_record), 0);
                let map = RunMap::from_instances(&instances, boot.cluster_bytes as u64);
                let covered = map.total_bytes().min(mft_real_size);
                if covered >= mft_real_size {
                    break map;
                }
                let covered_records = covered / record_size as u64;
                if covered_records <= parsed_up_to {
                    return Err(MftUnavailable::ReadFailed(
                        "could not map the whole $MFT".into(),
                    ));
                }
                while parsed_up_to < covered_records {
                    if let Some(record) =
                        read_one_record(&volume, &map, &boot, parsed_up_to, &mut scratch)?
                    {
                        records.push(record);
                    }
                    parsed_up_to += 1;
                }
            };

            Ok(VolumeReader {
                volume,
                boot,
                map,
                mft_real_size,
                bootstrap_records: records,
                parsed_up_to,
            })
        }

        /// Read and parse one record by number; `None` when it
        /// cannot be read or does not parse.
        pub(super) fn read_record(&mut self, ordinal: u64) -> Option<RawRecord> {
            let mut scratch = vec![0u8; self.boot.record_size as usize];
            read_one_record(&self.volume, &self.map, &self.boot, ordinal, &mut scratch)
                .ok()
                .flatten()
        }

        fn total_records(&self) -> u64 {
            self.mft_real_size / self.boot.record_size as u64
        }
    }

    pub fn scan_volume(root: &Path) -> Result<VolumeScan, MftUnavailable> {
        let prefix = volume_prefix(root).ok_or(MftUnavailable::NotVolumeRoot)?;
        let mut reader = VolumeReader::open(root)?;
        let boot = reader.boot;
        let record_size = boot.record_size as usize;

        // Stream the rest of the MFT in large chunks.
        let mut records = std::mem::take(&mut reader.bootstrap_records);
        let total_records = reader.total_records();
        let chunk_records = ((CHUNK / record_size) as u64).max(1);
        let mut scratch = vec![0u8; record_size];
        let mut ordinal = reader.parsed_up_to;
        while ordinal < total_records {
            let batch = chunk_records.min(total_records - ordinal);
            // Fast path: the batch lies inside one run segment —
            // read it in bulk. Otherwise fall back to per-record.
            let start_offset = ordinal * record_size as u64;
            let batch_bytes = batch * record_size as u64;
            let contiguous = reader.map.translate(start_offset).and_then(|first| {
                let last = reader.map.translate(start_offset + batch_bytes - 1)?;
                (last == first + batch_bytes - 1).then_some(first)
            });
            match contiguous {
                Some(volume_offset) => {
                    let mut buf = vec![0u8; batch_bytes as usize];
                    reader.volume.read_at(volume_offset, &mut buf)?;
                    for i in 0..batch {
                        let slice = &buf[i as usize * record_size..(i as usize + 1) * record_size];
                        if let Ok(Some(mut record)) = mft::parse_record(slice, &boot) {
                            record.number = ordinal + i;
                            records.push(record);
                        }
                    }
                }
                None => {
                    for i in 0..batch {
                        if let Some(record) = read_one_record(
                            &reader.volume,
                            &reader.map,
                            &boot,
                            ordinal + i,
                            &mut scratch,
                        )? {
                            records.push(record);
                        }
                    }
                }
            }
            ordinal += batch;
        }

        let (entries, stats) = mft::assemble_with_stats(&records);
        if entries.iter().filter(|e| !e.is_dir).count() == 0 {
            return Err(MftUnavailable::Empty);
        }
        let _ = stats; // assembly counters: useful in logs, not surfaced in v1
        Ok(anchor(entries, stats.skipped_reparse_paths, &prefix))
    }

    /// The record set the bootstrap merges over: what has been read
    /// so far, with record 0 guaranteed present even before the
    /// stream reaches it.
    fn records_with_mft(records: &[RawRecord], mft_record: &RawRecord) -> Vec<RawRecord> {
        if records.iter().any(|r| r.number == 0) {
            records.to_vec()
        } else {
            let mut all = vec![mft_record.clone()];
            all.extend(records.iter().cloned());
            all
        }
    }
}
