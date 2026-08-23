use std::path::Path;

use crate::domain::models::{AppUsage, EntryRecord, IndexStats, InstalledApp, SystemSnapshot};

pub trait SystemMonitor {
    fn snapshot(&mut self) -> anyhow::Result<SystemSnapshot>;
}

pub trait UsageProbe {
    fn probe(&self) -> anyhow::Result<Vec<AppUsage>>;
}

pub trait AppInventory {
    fn installed_apps(&self) -> anyhow::Result<Vec<InstalledApp>>;
}

/// removes a file or directory (implementations decide safety policy,
/// e.g. move to recycle bin)
pub trait PathRemover {
    fn remove_path(&self, path: &std::path::Path) -> anyhow::Result<()>;
}

pub trait RamTrimmer {
    /// trims working sets of the given pids; returns (succeeded, failed)
    fn trim_processes(&mut self, pids: &[u32]) -> anyhow::Result<(u32, u32)>;
    /// purges the standby list (needs elevation on Windows);
    /// Ok(false)/Err when unavailable
    fn purge_standby(&mut self) -> anyhow::Result<bool>;
}

pub trait RecycleBin {
    fn items(&self) -> anyhow::Result<Vec<crate::domain::models::BinItem>>;
    /// permanently deletes everything currently in the bin; returns count
    fn purge_all(&self) -> anyhow::Result<u64>;
}

pub trait IndexStore {
    fn get_dir_mtime(&self, path: &str) -> anyhow::Result<Option<i64>>;
    fn child_entries(&self, parent: &str) -> anyhow::Result<Vec<(String, bool)>>;
    fn upsert_entries(&mut self, entries: &[EntryRecord]) -> anyhow::Result<()>;
    fn delete_paths(&mut self, paths: &[String]) -> anyhow::Result<u64>;
    fn stats(&self) -> anyhow::Result<IndexStats>;
    fn clear(&mut self) -> anyhow::Result<()>;
    fn meta_get(&self, key: &str) -> anyhow::Result<Option<String>>;
    fn meta_set(&mut self, key: &str, value: &str) -> anyhow::Result<()>;
    /// all indexed files with size >= min_size, sorted by size desc
    fn files_by_size(&self, min_size: u64) -> anyhow::Result<Vec<(String, u64)>>;
}

pub trait PathNormalizer {
    fn normalize(&self, path: &Path) -> String;
}
