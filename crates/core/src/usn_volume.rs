//! Volume IO for the live index: reading the NTFS USN change
//! journal so file changes made since the last scan can be applied
//! to the in-memory index instead of waiting for the next scan.
//!
//! The parser (`usn`) is pure; this module is the thin,
//! Windows-only shell around it. It opens the raw volume exactly
//! like the MFT fast path does — which the OS only allows for
//! administrators — and when the process lacks access, or the
//! volume has no active journal, or anything at all goes wrong,
//! the caller gets a typed [`UsnUnavailable`] and the live index
//! quietly stays off. There is no elevation prompt, ever.
//!
//! A changed record carries only a name and a parent reference, so
//! paths are resolved by walking the parent chain's $FILE_NAME
//! records through the MFT (see [`crate::mft_volume::MftReader`]):
//! a record whose chain cannot be walked (parent deleted, chain
//! too deep, record unparsable) resolves to `None` and is counted
//! in [`ChangeBatch::unresolved`] — the caller treats an
//! incompletely resolved batch as not fully applicable.
//!
//! On non-Windows platforms every entry point reports
//! [`UsnUnavailable::Unsupported`].

use crate::usn::UsnRecord;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UsnUnavailable {
    #[error("the live index is only available on Windows")]
    Unsupported,
    #[error("could not open the volume (administrator rights are required): {0}")]
    OpenVolume(String),
    #[error("the volume's USN journal could not be read: {0}")]
    Ioctl(String),
}

/// Why a change read did not produce a batch.
#[derive(Debug, Error)]
pub enum UsnReadError {
    #[error(transparent)]
    Unavailable(#[from] UsnUnavailable),
    /// The journal no longer covers the requested starting point:
    /// it was deleted and recreated (a different journal id), or
    /// the checkpoint aged out of the journal's retained range
    /// (journal wrap). The changes in between are unknowable; the
    /// caller re-checkpoints at the journal's current end and
    /// treats its index as stale until the next scan reconciles it.
    #[error("the USN journal was reset or the checkpoint aged out of it")]
    JournalReset,
}

/// The state of a volume's USN journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalInfo {
    pub journal_id: u64,
    /// Oldest USN the journal still retains.
    pub first_usn: i64,
    /// USN the next change will receive: the freshness point a
    /// checkpoint taken "now" stores.
    pub next_usn: i64,
    pub max_usn: i64,
}

/// One journal record with its resolved full path (`None` when the
/// parent chain could not be walked).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedChange {
    pub record: UsnRecord,
    pub path: Option<String>,
}

/// The result of one journal read: every change since the
/// checkpoint, in journal order, plus where the next read resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeBatch {
    pub changes: Vec<ResolvedChange>,
    /// The USN to checkpoint: the first USN not covered by this
    /// batch.
    pub next_usn: i64,
    /// How many records' paths could not be resolved.
    pub unresolved: usize,
}

/// Records read per poll at most. A batch that hits the cap simply
/// leaves the checkpoint mid-stream; the next poll continues.
pub const MAX_RECORDS_PER_POLL: usize = 200_000;

/// Query a volume's USN journal state. `volume_root` is the
/// volume's root path (`C:\`); the device is derived from its
/// drive prefix.
pub fn query_journal(volume_root: &Path) -> Result<JournalInfo, UsnUnavailable> {
    #[cfg(windows)]
    {
        imp::query_journal(volume_root)
    }
    #[cfg(not(windows))]
    {
        let _ = volume_root;
        Err(UsnUnavailable::Unsupported)
    }
}

/// Read every journal change since `start_usn` in the journal
/// identified by `journal_id`, resolving each record's full path
/// through the MFT.
///
/// Returns [`UsnReadError::JournalReset`] when the live journal is
/// not the checkpointed one, or no longer reaches back to
/// `start_usn`.
pub fn read_changes(
    volume_root: &Path,
    journal_id: u64,
    start_usn: i64,
) -> Result<ChangeBatch, UsnReadError> {
    #[cfg(windows)]
    {
        imp::read_changes(volume_root, journal_id, start_usn)
    }
    #[cfg(not(windows))]
    {
        let _ = (volume_root, journal_id, start_usn);
        Err(UsnReadError::Unavailable(UsnUnavailable::Unsupported))
    }
}

#[cfg(windows)]
mod imp {
    use super::{
        ChangeBatch, JournalInfo, ResolvedChange, UsnReadError, UsnUnavailable,
        MAX_RECORDS_PER_POLL,
    };
    use crate::mft::{self, FileName, RawRecord};
    use crate::mft_volume::{self, MftReader};
    use crate::usn::{self, UsnRecord};
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
        USN_JOURNAL_DATA_V0,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    /// Output buffer per FSCTL_READ_USN_JOURNAL call.
    const READ_BUFFER: usize = 1 << 20;

    struct Volume(HANDLE);

    impl Drop for Volume {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn open_volume(volume_root: &Path) -> Result<Volume, UsnUnavailable> {
        let prefix = mft_volume::volume_prefix(volume_root)
            .ok_or_else(|| UsnUnavailable::OpenVolume("not a drive path".into()))?;
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
        .map_err(|e| UsnUnavailable::OpenVolume(format!("{device}: {e}")))?;
        Ok(Volume(handle))
    }

    fn query_on(volume: &Volume) -> Result<JournalInfo, UsnUnavailable> {
        let mut data: USN_JOURNAL_DATA_V0 = unsafe { std::mem::zeroed() };
        let mut returned: u32 = 0;
        unsafe {
            DeviceIoControl(
                volume.0,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(&mut data as *mut USN_JOURNAL_DATA_V0 as *mut std::ffi::c_void),
                std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
                Some(&mut returned),
                None,
            )
        }
        .map_err(|e| UsnUnavailable::Ioctl(format!("FSCTL_QUERY_USN_JOURNAL: {e}")))?;
        Ok(JournalInfo {
            journal_id: data.UsnJournalID,
            first_usn: data.FirstUsn,
            next_usn: data.NextUsn,
            max_usn: data.MaxUsn,
        })
    }

    pub fn query_journal(volume_root: &Path) -> Result<JournalInfo, UsnUnavailable> {
        let volume = open_volume(volume_root)?;
        query_on(&volume)
    }

    pub fn read_changes(
        volume_root: &Path,
        journal_id: u64,
        start_usn: i64,
    ) -> Result<ChangeBatch, UsnReadError> {
        let volume = open_volume(volume_root)?;
        let info = query_on(&volume)?;
        // The checkpoint must belong to this journal and still be
        // inside its retained range; anything else means changes
        // were lost and only a fresh scan can reconcile them.
        if info.journal_id != journal_id
            || start_usn < info.first_usn
            || start_usn > info.next_usn
        {
            return Err(UsnReadError::JournalReset);
        }
        // Path resolution reads MFT records; without that reader
        // the batch would be all-unresolved, so fail the same way
        // the scan's fast path does and let the caller retry later.
        let mut reader = mft_volume::open_reader(volume_root)
            .map_err(|e| UsnReadError::Unavailable(UsnUnavailable::Ioctl(e.to_string())))?;
        let prefix = mft_volume::volume_prefix(volume_root)
            .ok_or_else(|| UsnUnavailable::OpenVolume("not a drive path".into()))?;

        let mut records: Vec<UsnRecord> = Vec::new();
        let mut next = start_usn;
        loop {
            if records.len() >= MAX_RECORDS_PER_POLL {
                break;
            }
            let input = READ_USN_JOURNAL_DATA_V0 {
                StartUsn: next,
                ReasonMask: usn::READ_REASON_MASK,
                ReturnOnlyOnClose: 0,
                Timeout: 0,
                BytesToWaitFor: 0,
                UsnJournalID: journal_id,
            };
            let mut out = vec![0u8; READ_BUFFER];
            let mut returned: u32 = 0;
            let call = unsafe {
                DeviceIoControl(
                    volume.0,
                    FSCTL_READ_USN_JOURNAL,
                    Some(&input as *const READ_USN_JOURNAL_DATA_V0 as *const std::ffi::c_void),
                    std::mem::size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                    Some(out.as_mut_ptr() as *mut std::ffi::c_void),
                    out.len() as u32,
                    Some(&mut returned),
                    None,
                )
            };
            if let Err(e) = call {
                // A journal deleted mid-read is a reset; anything
                // else with a partial batch in hand keeps the batch
                // (the header USN it ends at stays checkpointable),
                // and with nothing in hand is simply unavailable.
                let code = e.code().0 as u32;
                const ERROR_JOURNAL_DELETE_IN_PROGRESS: u32 = 1180;
                const ERROR_JOURNAL_ENTRY_DELETED: u32 = 1181;
                if code == ERROR_JOURNAL_DELETE_IN_PROGRESS || code == ERROR_JOURNAL_ENTRY_DELETED
                {
                    return Err(UsnReadError::JournalReset);
                }
                if records.is_empty() {
                    return Err(UsnReadError::Unavailable(UsnUnavailable::Ioctl(format!(
                        "FSCTL_READ_USN_JOURNAL: {e}"
                    ))));
                }
                break;
            }
            let returned = (returned as usize).min(out.len());
            if returned <= 8 {
                // Only the next-USN header: caught up.
                if returned == 8 {
                    next = i64::from_le_bytes(out[0..8].try_into().unwrap());
                }
                break;
            }
            let header_next = i64::from_le_bytes(out[0..8].try_into().unwrap());
            let parsed = usn::parse_usn_records(&out[8..returned]);
            let advanced = header_next > next;
            next = header_next;
            records.extend(parsed);
            if !advanced {
                // The journal claims no forward motion; do not spin.
                break;
            }
        }
        let next_usn = next.max(start_usn);

        let mut resolver = Resolver {
            reader: &mut reader,
            prefix: &prefix,
            dir_cache: HashMap::new(),
        };
        let mut unresolved = 0usize;
        let changes = records
            .into_iter()
            .map(|record| {
                let path = resolver.resolve(&record);
                if path.is_none() {
                    unresolved += 1;
                }
                ResolvedChange { record, path }
            })
            .collect();
        Ok(ChangeBatch {
            changes,
            next_usn,
            unresolved,
        })
    }

    /// Parent-chain path resolution over single MFT record reads.
    ///
    /// Directory paths are cached per batch (most changes share a
    /// handful of parents, and each uncached directory costs one
    /// record read per ancestor). The walk is capped at 32 levels;
    /// NTFS paths deeper than that do not occur in practice, and a
    /// cycle or a missing/deleted ancestor fails the resolution
    /// rather than looping.
    struct Resolver<'a> {
        reader: &'a mut MftReader,
        /// Canonical volume prefix (`C:\`) paths anchor at.
        prefix: &'a str,
        dir_cache: HashMap<u64, Option<String>>,
    }

    impl Resolver<'_> {
        /// Full path of the file `record` describes: its parent's
        /// path plus its own name.
        fn resolve(&mut self, record: &UsnRecord) -> Option<String> {
            let parent = self.dir_path(record.parent_ref)?;
            if parent.ends_with('\\') {
                Some(format!("{parent}{}", record.name))
            } else {
                Some(format!("{parent}\\{}", record.name))
            }
        }

        /// Full path of the directory with the given record number.
        fn dir_path(&mut self, number: u64) -> Option<String> {
            if number == mft::ROOT_RECORD {
                return Some(self.prefix.to_string());
            }
            if let Some(cached) = self.dir_cache.get(&number) {
                return cached.clone();
            }
            let resolved = self.walk(number);
            self.dir_cache.insert(number, resolved.clone());
            resolved
        }

        fn walk(&mut self, number: u64) -> Option<String> {
            let mut segments: Vec<String> = Vec::new();
            let mut visited: HashSet<u64> = HashSet::new();
            let mut cur = number;
            for _ in 0..32 {
                if cur == mft::ROOT_RECORD {
                    let base = self.prefix.trim_end_matches('\\');
                    let mut path = base.to_string();
                    for seg in segments.iter().rev() {
                        path.push('\\');
                        path.push_str(seg);
                    }
                    return Some(path);
                }
                if !visited.insert(cur) {
                    return None; // parent cycle
                }
                let record = self.reader.read_record(cur)?;
                if !record.in_use || !record.is_directory {
                    return None;
                }
                let names = self.merged_names(&record);
                let links = mft::record_links(&names);
                let (parent, link) = links.into_iter().min_by(|a, b| {
                    mft::namespace_rank(a.1.namespace)
                        .cmp(&mft::namespace_rank(b.1.namespace))
                        .then(a.1.name.cmp(&b.1.name))
                })?;
                segments.push(link.name.clone());
                cur = parent;
            }
            None // deeper than the cap
        }

        /// The $FILE_NAME attributes that describe `record`,
        /// gathered the way assembly merges them: an extension
        /// record's names live with its base, and an attribute list
        /// can name further records holding $FILE_NAME instances.
        /// Directories are almost never fragmented, so in practice
        /// this is the record's own names; the extra reads keep the
        /// unusual cases resolvable instead of skipped.
        fn merged_names(&mut self, record: &RawRecord) -> Vec<FileName> {
            let mut names: Vec<FileName> = record.names.clone();
            let mut extra: Vec<u64> = Vec::new();
            if record.is_extension() {
                extra.push(record.base_ref);
            }
            for entry in &record.attr_list {
                if entry.attr_type == 0x30 && entry.record_ref != record.number {
                    extra.push(entry.record_ref);
                }
            }
            for number in extra {
                if let Some(other) = self.reader.read_record(number) {
                    names.extend(other.names.iter().cloned());
                    for entry in &other.attr_list {
                        if entry.attr_type == 0x30 && entry.record_ref != number {
                            if let Some(deeper) = self.reader.read_record(entry.record_ref) {
                                names.extend(deeper.names.iter().cloned());
                            }
                        }
                    }
                }
            }
            names
        }
    }
}
