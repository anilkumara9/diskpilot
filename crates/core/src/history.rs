//! SQLite history: every scan and every cleanup action is recorded locally.

use crate::error::CoreResult;
use crate::model::{CleanupResult, HistoryEntry, ScanSummary};
use rusqlite::{params, Connection};

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

    pub fn record_cleanup(
        &self,
        operation: &str,
        result: &CleanupResult,
        detail: Option<&str>,
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
                "INSERT INTO cleanup_items (action_id, path) VALUES (?1, ?2)",
                params![action_id, path],
            )?;
        }
        Ok(action_id)
    }

    pub fn record_action(
        &self,
        timestamp_ms: i64,
        operation: &str,
        paths: &[String],
        reclaimed_bytes: u64,
        errors: &[String],
        detail: Option<&str>,
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
                "INSERT INTO cleanup_items (action_id, path) VALUES (?1, ?2)",
                params![action_id, path],
            )?;
        }
        Ok(action_id)
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
