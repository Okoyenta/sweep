use std::path::Path;

use anyhow::Context;
use rusqlite::{params, Connection};

use crate::domain::models::{EntryRecord, IndexStats};
use crate::domain::traits::IndexStore;

const SCHEMA_VERSION: &str = "1";

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating data dir {}", dir.display()))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("opening index db at {}", db_path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS entries (
                 path   TEXT PRIMARY KEY,
                 parent TEXT NOT NULL,
                 size   INTEGER NOT NULL,
                 mtime  INTEGER NOT NULL,
                 is_dir INTEGER NOT NULL
             ) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS idx_entries_parent ON entries(parent);
             CREATE TABLE IF NOT EXISTS meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             ) WITHOUT ROWID;
             COMMIT;",
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION],
        )?;
        Ok(())
    }
}

impl IndexStore for SqliteStore {
    fn get_dir_mtime(&self, path: &str) -> anyhow::Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT mtime FROM entries WHERE path = ?1 AND is_dir = 1")?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn child_entries(&self, parent: &str) -> anyhow::Result<Vec<(String, bool)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT path, is_dir FROM entries WHERE parent = ?1")?;
        let rows = stmt.query_map(params![parent], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn upsert_entries(&mut self, entries: &[EntryRecord]) -> anyhow::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO entries(path, parent, size, mtime, is_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                     parent = excluded.parent,
                     size = excluded.size,
                     mtime = excluded.mtime,
                     is_dir = excluded.is_dir",
            )?;
            for e in entries {
                stmt.execute(params![
                    e.path,
                    e.parent,
                    e.size_bytes as i64,
                    e.mtime_ms,
                    e.is_dir as i64
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn delete_paths(&mut self, paths: &[String]) -> anyhow::Result<u64> {
        if paths.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut deleted = 0u64;
        {
            let mut stmt = tx.prepare_cached("DELETE FROM entries WHERE path = ?1")?;
            for p in paths {
                deleted += stmt.execute(params![p])? as u64;
            }
        }
        tx.commit()?;
        Ok(deleted)
    }

    fn stats(&self) -> anyhow::Result<IndexStats> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT
                COALESCE(SUM(CASE WHEN is_dir = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_dir = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_dir = 0 THEN size ELSE 0 END), 0)
             FROM entries",
        )?;
        let mut rows = stmt.query([])?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("stats query returned no rows"))?;
        let files: i64 = row.get(0)?;
        let dirs: i64 = row.get(1)?;
        let total_bytes: i64 = row.get(2)?;
        Ok(IndexStats {
            files: files.max(0) as u64,
            dirs: dirs.max(0) as u64,
            total_bytes: total_bytes.max(0) as u64,
        })
    }

    fn clear(&mut self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "BEGIN; DELETE FROM entries; DELETE FROM meta WHERE key != 'schema_version'; COMMIT;",
        )?;
        Ok(())
    }

    fn meta_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn meta_set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn files_by_size(&self, min_size: u64) -> anyhow::Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, size FROM entries
             WHERE is_dir = 0 AND size >= ?1
             ORDER BY size DESC",
        )?;
        let rows = stmt.query_map(params![min_size as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent_of(path: &str) -> Option<String> {
        Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty())
    }

    fn rec(path: &str, size: u64, mtime: i64, is_dir: bool) -> EntryRecord {
        EntryRecord {
            path: path.to_string(),
            parent: parent_of(path).unwrap_or_default(),
            size_bytes: size,
            mtime_ms: mtime,
            is_dir,
        }
    }

    #[test]
    fn upsert_query_delete_roundtrip() -> anyhow::Result<()> {
        let mut store = SqliteStore::open_in_memory()?;
        store.upsert_entries(&[
            rec("C:/data", 0, 100, true),
            rec("C:/data/a.txt", 10, 100, false),
            rec("C:/data/b.txt", 20, 110, false),
        ])?;

        assert_eq!(store.get_dir_mtime("C:/data")?, Some(100));
        assert_eq!(store.get_dir_mtime("C:/missing")?, None);

        let mut children: Vec<String> = store
            .child_entries("C:/data")?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        children.sort();
        assert_eq!(children, vec!["C:/data/a.txt".to_string(), "C:/data/b.txt".to_string()]);
        assert!(store.child_entries("C:/nothing")?.is_empty());

        store.upsert_entries(&[rec("C:/data/nested", 0, 90, true)])?;
        let nested = store.child_entries("C:/data")?;
        assert!(nested.contains(&("C:/data/nested".to_string(), true)));

        let stats = store.stats()?;
        assert_eq!(stats.files, 2);
        assert_eq!(stats.dirs, 2);
        assert_eq!(stats.total_bytes, 30);

        store.upsert_entries(&[rec("C:/data/a.txt", 15, 200, false)])?;
        assert_eq!(store.stats()?.total_bytes, 35);
        assert_eq!(store.get_dir_mtime("C:/data/a.txt")?, None);

        let removed = store.delete_paths(&["C:/data/b.txt".to_string()])?;
        assert_eq!(removed, 1);
        let stats = store.stats()?;
        assert_eq!(stats.files, 1);
        assert_eq!(stats.total_bytes, 15);
        assert_eq!(removed, 1);

        store.meta_set("last_run", "123")?;
        assert_eq!(store.meta_get("last_run")?, Some("123".to_string()));
        assert_eq!(store.meta_get("nope")?, None);

        store.clear()?;
        let stats = store.stats()?;
        assert_eq!(stats.files + stats.dirs, 0);
        Ok(())
    }
}
