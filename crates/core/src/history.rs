//! SQLite history: every scan and every cleanup action is recorded locally.
//!
//! Each recorded cleanup item also carries how it was removed
//! ([`RemovedItemMode`]), which is what makes a whole cleanup run
//! restorable later: Trash-mode items can be put back from the
//! Recycle Bin, permanent ones cannot. Items recorded before the
//! mode column existed have no mode (`NULL`) and are never treated
//! as restorable.

use crate::error::CoreResult;
use crate::model::{CleanupResult, HistoryEntry, ScanSummary};
use rusqlite::{params, Connection};

/// How one recorded cleanup item was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovedItemMode {
    /// Moved to the Recycle Bin — restorable while it sits there.
    Trash,
    /// Deleted outright — gone for good.
    Permanent,
}

impl RemovedItemMode {
    fn as_str(&self) -> &'static str {
        match self {
            RemovedItemMode::Trash => "trash",
            RemovedItemMode::Permanent => "permanent",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "trash" => Some(RemovedItemMode::Trash),
            "permanent" => Some(RemovedItemMode::Permanent),
            _ => None,
        }
    }
}

/// One item of a recorded cleanup action, with its removal mode.
/// `mode` is `None` for items recorded before modes were tracked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupItemRecord {
    pub path: String,
    pub mode: Option<RemovedItemMode>,
}

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: &std::path::Path) -> CoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> CoreResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                root TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                finished_at_ms INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                dir_count INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cleanup_actions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                operation TEXT NOT NULL,
                reclaimed_bytes INTEGER NOT NULL,
                errors TEXT NOT NULL DEFAULT '[]',
                detail TEXT
            );
            CREATE TABLE IF NOT EXISTS cleanup_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action_id INTEGER NOT NULL REFERENCES cleanup_actions(id),
                path TEXT NOT NULL
            );",
        )?;
        // Mode column, added after the table shipped: databases
        // written before it existed keep their rows with a NULL mode
        // (unknown removal mode — never restorable, see the module
        // docs). Guarded by a pragma check so re-runs are no-ops.
        let has_mode: bool = {
            let mut stmt = self.conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('cleanup_items') WHERE name = 'mode'",
            )?;
            stmt.query_row([], |row| row.get::<_, i64>(0))? > 0
        };
        if !has_mode {
            self.conn
                .execute_batch("ALTER TABLE cleanup_items ADD COLUMN mode TEXT;")?;
        }
        Ok(())
    }

    pub fn record_scan(&self, summary: &ScanSummary) -> CoreResult<i64> {
        self.conn.execute(
            "INSERT INTO scans (root, started_at_ms, finished_at_ms, total_bytes, file_count, dir_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                summary.root,
                summary.started_at_ms,
                summary.finished_at_ms,
                summary.total_bytes as i64,
                summary.file_count as i64,
                summary.dir_count as i64
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Record one cleanup execution. Every succeeded item is stored
    /// with the run's removal `mode` (a single execution uses one
    /// mode throughout), which is what later lets the run be
    /// restored as a group.
    // The recorder's fields are one flat audit record; bundling them
    // into a struct would obscure call sites without adding safety.
    #[allow(clippy::too_many_arguments)]
    pub fn record_cleanup(
        &self,
        operation: &str,
        result: &CleanupResult,
        detail: Option<&str>,
        mode: RemovedItemMode,
    ) -> CoreResult<i64> {
        let errors: Vec<String> = result
            .failed
            .iter()
            .map(|f| format!("{}: {}", f.path, f.message))
            .collect();
        self.conn.execute(
            "INSERT INTO cleanup_actions (timestamp_ms, operation, reclaimed_bytes, errors, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                result.finished_at_ms,
                operation,
                result.reclaimed_bytes as i64,
                serde_json::to_string(&errors)?,
                detail
            ],
        )?;
        let action_id = self.conn.last_insert_rowid();
        for path in &result.succeeded {
            self.conn.execute(
                "INSERT INTO cleanup_items (action_id, path, mode) VALUES (?1, ?2, ?3)",
                params![action_id, path, mode.as_str()],
            )?;
        }
        Ok(action_id)
    }

    /// Record a free-form action (e.g. restoring a cleanup run) with
    /// its affected paths, each stamped with `mode` like
    /// [`HistoryStore::record_cleanup`] items.
    #[allow(clippy::too_many_arguments)]
    pub fn record_action(
        &self,
        timestamp_ms: i64,
        operation: &str,
        paths: &[String],
        reclaimed_bytes: u64,
        errors: &[String],
        detail: Option<&str>,
        mode: RemovedItemMode,
    ) -> CoreResult<i64> {
        self.conn.execute(
            "INSERT INTO cleanup_actions (timestamp_ms, operation, reclaimed_bytes, errors, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                timestamp_ms,
                operation,
                reclaimed_bytes as i64,
                serde_json::to_string(errors)?,
                detail
            ],
        )?;
        let action_id = self.conn.last_insert_rowid();
        for path in paths {
            self.conn.execute(
                "INSERT INTO cleanup_items (action_id, path, mode) VALUES (?1, ?2, ?3)",
                params![action_id, path, mode.as_str()],
            )?;
        }
        Ok(action_id)
    }

    /// The recorded items of one cleanup action, oldest first.
    /// `Ok(None)` when no action with `entry_id` exists; an existing
    /// action's items may carry `mode: None` when they were recorded
    /// before removal modes were tracked.
    pub fn entry_items(&self, entry_id: i64) -> CoreResult<Option<Vec<CleanupItemRecord>>> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM cleanup_actions WHERE id = ?1",
            params![entry_id],
            |row| row.get::<_, i64>(0),
        )? > 0;
        if !exists {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT path, mode FROM cleanup_items WHERE action_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![entry_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (path, mode) = row?;
            items.push(CleanupItemRecord {
                path,
                mode: mode.as_deref().and_then(RemovedItemMode::from_str),
            });
        }
        Ok(Some(items))
    }

    pub fn list_history(&self, limit: usize) -> CoreResult<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp_ms, operation, reclaimed_bytes, errors, detail
             FROM cleanup_actions ORDER BY timestamp_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, ts, op, reclaimed, errors_json, detail) = row?;
            let mut paths_stmt = self
                .conn
                .prepare("SELECT path FROM cleanup_items WHERE action_id = ?1 ORDER BY id")?;
            let paths: Vec<String> = paths_stmt
                .query_map(params![id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            out.push(HistoryEntry {
                id,
                timestamp_ms: ts,
                operation: op,
                paths,
                reclaimed_bytes: reclaimed.max(0) as u64,
                errors: serde_json::from_str(&errors_json).unwrap_or_default(),
                detail,
                // Filled in by the shell, which owns the Recycle Bin.
                restorable_count: 0,
            });
        }
        Ok(out)
    }

    pub fn scan_count(&self) -> CoreResult<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM scans", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn total_reclaimed(&self) -> CoreResult<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(reclaimed_bytes),0) FROM cleanup_actions",
            [],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as u64)
    }
}

pub fn default_db_path() -> std::path::PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("DiskPilot").join("history.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CleanupResult;

    fn result(succeeded: &[&str]) -> CleanupResult {
        CleanupResult {
            plan_id: "plan-1".to_string(),
            succeeded: succeeded.iter().map(|s| s.to_string()).collect(),
            failed: Vec::new(),
            reclaimed_bytes: 42,
            finished_at_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn recorded_items_carry_the_run_mode() {
        let store = HistoryStore::in_memory().unwrap();
        let trash_id = store
            .record_cleanup(
                "cleanup",
                &result(&["C:\\a.tmp", "C:\\b.tmp"]),
                None,
                RemovedItemMode::Trash,
            )
            .unwrap();
        let items = store.entry_items(trash_id).unwrap().unwrap();
        assert_eq!(
            items,
            vec![
                CleanupItemRecord {
                    path: "C:\\a.tmp".to_string(),
                    mode: Some(RemovedItemMode::Trash),
                },
                CleanupItemRecord {
                    path: "C:\\b.tmp".to_string(),
                    mode: Some(RemovedItemMode::Trash),
                },
            ]
        );

        let perm_id = store
            .record_cleanup(
                "cleanup",
                &result(&["C:\\c.tmp"]),
                None,
                RemovedItemMode::Permanent,
            )
            .unwrap();
        let items = store.entry_items(perm_id).unwrap().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].mode, Some(RemovedItemMode::Permanent));
    }

    #[test]
    fn record_action_stamps_modes_too() {
        let store = HistoryStore::in_memory().unwrap();
        let id = store
            .record_action(
                1_700_000_000_000,
                "restore",
                &["C:\\back.tmp".to_string()],
                0,
                &[],
                Some("Restored 1 items from a cleanup"),
                RemovedItemMode::Trash,
            )
            .unwrap();
        let items = store.entry_items(id).unwrap().unwrap();
        assert_eq!(items[0].mode, Some(RemovedItemMode::Trash));
    }

    #[test]
    fn entry_items_for_unknown_id_is_none() {
        let store = HistoryStore::in_memory().unwrap();
        assert_eq!(store.entry_items(999).unwrap(), None);
        // An action with no succeeded items is Some(empty), not None.
        let id = store
            .record_cleanup("cleanup", &result(&[]), None, RemovedItemMode::Trash)
            .unwrap();
        assert_eq!(store.entry_items(id).unwrap(), Some(Vec::new()));
    }

    #[test]
    fn pre_mode_rows_migrate_to_unknown_mode() {
        // Build a database in the pre-mode shape by hand: the
        // cleanup_items table without the mode column, one action
        // and one item already recorded.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE cleanup_actions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp_ms INTEGER NOT NULL,
                    operation TEXT NOT NULL,
                    reclaimed_bytes INTEGER NOT NULL,
                    errors TEXT NOT NULL DEFAULT '[]',
                    detail TEXT
                );
                CREATE TABLE cleanup_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    action_id INTEGER NOT NULL REFERENCES cleanup_actions(id),
                    path TEXT NOT NULL
                );
                INSERT INTO cleanup_actions (timestamp_ms, operation, reclaimed_bytes, errors)
                VALUES (1700000000000, 'cleanup', 7, '[]');
                INSERT INTO cleanup_items (action_id, path) VALUES (1, 'C:\\old.tmp');",
            )
            .unwrap();
        }
        // Opening through the store adds the column via migration…
        let store = HistoryStore::open(&path).unwrap();
        let items = store.entry_items(1).unwrap().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, "C:\\old.tmp");
        assert_eq!(
            items[0].mode, None,
            "pre-mode rows keep an unknown removal mode"
        );
        // …and new recordings on the migrated database carry modes.
        let id = store
            .record_cleanup(
                "cleanup",
                &result(&["C:\\new.tmp"]),
                None,
                RemovedItemMode::Trash,
            )
            .unwrap();
        assert_eq!(
            store.entry_items(id).unwrap().unwrap()[0].mode,
            Some(RemovedItemMode::Trash)
        );
    }
}
