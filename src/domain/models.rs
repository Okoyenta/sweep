use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DiskStats {
    pub name: String,
    pub mount_point: PathBuf,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ProcessMemInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub memory: MemoryStats,
    pub disks: Vec<DiskStats>,
    pub top_processes: Vec<ProcessMemInfo>,
}

#[derive(Debug, Clone)]
pub struct EntryRecord {
    pub path: String,
    pub parent: String,
    pub size_bytes: u64,
    pub mtime_ms: i64,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct DirListing {
    pub path: String,
    pub dir_mtime_ms: i64,
    pub readable: bool,
    pub entries: Vec<EntryRecord>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexStats {
    pub files: u64,
    pub dirs: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct IndexProgress {
    pub running: bool,
    pub dirs_scanned: u64,
    pub dirs_skipped: u64,
    pub files_recorded: u64,
    pub stale_removed: u64,
    pub errors: u64,
    pub current_path: String,
    pub started_at_unix: Option<i64>,
    pub finished_at_unix: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSource {
    Prefetch,
    UserAssist,
}

#[derive(Debug, Clone)]
pub struct AppUsage {
    pub exe_name: String,
    pub last_run_unix: i64,
    pub run_count: u64,
    pub source: UsageSource,
}

#[derive(Debug, Clone, Default)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: Option<String>,
    pub uninstall_command: Option<String>,
    pub size_bytes: Option<u64>,
    pub last_run_unix: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CleanCategory {
    pub id: String,
    pub title: String,
    /// each root's children are removal candidates; a root that is a plain
    /// file is itself the candidate
    pub roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CategoryScan {
    pub category_id: String,
    pub title: String,
    pub items: Vec<PathBuf>,
    pub total_bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CleanOutcome {
    pub removed_bytes: u64,
    pub removed_items: u64,
    pub failed_items: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TrimOutcome {
    pub attempted_pids: Vec<u32>,
    pub succeeded: u32,
    pub failed: u32,
    pub standby_attempted: bool,
    pub standby_ok: bool,
}

#[derive(Debug, Clone)]
pub struct BinItem {
    pub name: String,
    pub original_parent: String,
    pub deleted_unix: i64,
}
