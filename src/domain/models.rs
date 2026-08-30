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
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub total_written_bytes: u64,
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
    pub risk: RiskLevel,
    /// when set, sweep runs this shell command instead of trashing files
    /// (used for pnpm store, which uses hardlinks that don't free on trash)
    pub cleanup_command: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CategoryScan {
    pub category_id: String,
    pub title: String,
    pub items: Vec<PathBuf>,
    pub total_bytes: u64,
    pub files: u64,
    /// copied from CleanCategory; run this command instead of trashing if set
    pub cleanup_command: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CleanOutcome {
    pub removed_bytes: u64,
    pub removed_items: u64,
    pub failed_items: u64,
    /// paths that could not be removed (locked or protected), for the
    /// `clean --kill` retry pass
    pub failed_paths: Vec<PathBuf>,
    /// successfully trashed items, recorded so the caller can append an
    /// [`UndoSession`] to the journal for `sweep undo`
    pub undo_items: Vec<UndoItem>,
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

pub const RESERVE_SIZE_BYTES: u64 = 512 * 1024 * 1024;

pub const HEADROOM_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;

pub const RECREATION_THRESHOLD_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Safe,
    System,
}

#[derive(Debug, Clone)]
pub struct DiagnoseRow {
    pub category_id: String,
    pub title: String,
    pub size_bytes: u64,
    pub risk: RiskLevel,
    pub reclaimable: bool,
    /// actionable guidance shown below the row (e.g. DISM command for driver store)
    pub hint: Option<String>,
}

/// a running process that holds an open handle on one or more cleanable
/// paths (discovered via Restart Manager on Windows or a name heuristic on
/// Linux); used by `clean --kill`
#[derive(Debug, Clone)]
pub struct LockedProcess {
    pub pid: u32,
    /// image name, e.g. "chrome.exe"
    pub name: String,
    /// the specific locked path this process holds
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct IdleProbeResult {
    pub idle_seconds: u64,
    pub foreground_title: String,
}

#[derive(Debug, Clone)]
pub struct DiagnoseReport {
    pub rows: Vec<DiagnoseRow>,
    pub total_reclaimable: u64,
    pub safe_reclaimable: u64,
    pub system_reclaimable: u64,
    pub idle: Option<IdleProbeResult>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkSample {
    pub before_free_bytes: u64,
    pub after_free_bytes: u64,
    pub elapsed_secs: f64,
    pub category_bytes: Vec<(String, u64)>,
}

impl BenchmarkSample {
    pub fn freed_bytes(&self) -> u64 {
        self.after_free_bytes.saturating_sub(self.before_free_bytes)
    }

    pub fn safe_freed(&self, categories: &[CleanCategory]) -> u64 {
        self.category_bytes
            .iter()
            .filter(|(id, _)| {
                categories
                    .iter()
                    .any(|c| c.id == *id && c.risk == RiskLevel::Safe)
            })
            .map(|(_, b)| b)
            .sum()
    }

    pub fn system_freed(&self, categories: &[CleanCategory]) -> u64 {
        self.category_bytes
            .iter()
            .filter(|(id, _)| {
                categories
                    .iter()
                    .any(|c| c.id == *id && c.risk == RiskLevel::System)
            })
            .map(|(_, b)| b)
            .sum()
    }
}

pub const DEFAULT_GUARD_RAM_THRESHOLD: f64 = 0.90;
pub const DEFAULT_GUARD_DISK_MIN_GB: u64 = 10;
pub const DEFAULT_GUARD_INTERVAL_SECS: u64 = 60;
pub const GUARD_COOLDOWN_SECS: u64 = 300;
pub const GUARD_HYSTERESIS_SAMPLES: usize = 3;

/// Configuration for the guard daemon.
#[derive(Debug, Clone)]
pub struct GuardConfig {
    pub ram_threshold: f64,
    pub disk_min_gb: u64,
    pub interval_secs: u64,
    pub once: bool,
    pub allow_service_stop: bool,
    pub allow_kill: bool,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            ram_threshold: DEFAULT_GUARD_RAM_THRESHOLD,
            disk_min_gb: DEFAULT_GUARD_DISK_MIN_GB,
            interval_secs: DEFAULT_GUARD_INTERVAL_SECS,
            once: false,
            allow_service_stop: false,
            allow_kill: false,
        }
    }
}

/// RAM usage snapshot at a point in time.
#[derive(Debug, Clone)]
pub struct RamSnapshot {
    pub timestamp_secs: i64,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_pct: f64,
}

/// Disk free space snapshot at a point in time.
#[derive(Debug, Clone)]
pub struct DiskSnapshot {
    pub timestamp_secs: i64,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// RAII state for tracking which services were running before guard intervention.
#[derive(Debug, Clone)]
pub struct ServiceGuardState {
    pub name: String,
    pub was_running: bool,
}

#[derive(Debug, Clone)]
pub struct ServiceEntry {
    pub name: String,
    pub display_name: String,
    pub pid: Option<u32>,
    pub status: String,
}

/// A process detected as idle but writing heavily to disk.
#[derive(Debug, Clone)]
pub struct IdleSsdOffender {
    pub pid: u32,
    pub name: String,
    pub idle_secs: u64,
    pub write_bytes: u64,
    pub writes_per_hour: f64,
    pub memory_bytes: u64,
    pub reason: IdleReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleReason {
    BackgroundFlush,
    SyncService,
    Unknown,
}

impl std::fmt::Display for IdleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdleReason::BackgroundFlush => write!(f, "Background flush"),
            IdleReason::SyncService => write!(f, "Sync service"),
            IdleReason::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuardBenchmark {
    pub phase: String,
    pub before_free_bytes: u64,
    pub after_free_bytes: u64,
    pub freed_bytes: u64,
    pub elapsed_secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardAction {
    None,
    RamTrim,
    DiskReserve,
    DiskCleanSafe,
    DiskPurgeBin,
}

impl std::fmt::Display for GuardAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardAction::None => write!(f, "none"),
            GuardAction::RamTrim => write!(f, "ram_trim"),
            GuardAction::DiskReserve => write!(f, "disk_reserve"),
            GuardAction::DiskCleanSafe => write!(f, "disk_clean_safe"),
            GuardAction::DiskPurgeBin => write!(f, "disk_purge_bin"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskRescuePhase {
    Reserve,
    CleanSafe,
    PurgeBin,
}

/// Timestamped log entry written by the guard daemon.
#[derive(Debug, Clone)]
pub struct GuardLogEntry {
    pub timestamp: String,
    pub level: GuardLogLevel,
    pub message: String,
    pub bytes_freed: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardLogLevel {
    Info,
    Warn,
    Action,
}

impl std::fmt::Display for GuardLogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardLogLevel::Info => write!(f, "INFO"),
            GuardLogLevel::Warn => write!(f, "WARN"),
            GuardLogLevel::Action => write!(f, "ACTION"),
        }
    }
}

/// Result of a deep system scan (WU, DO, WinSxS, driver store).
#[derive(Debug, Clone)]
pub struct DeepScanResult {
    pub wu_download_bytes: u64,
    pub do_cache_bytes: u64,
    pub winsxs_reclaimable_bytes: Option<u64>,
    pub driver_store_bytes: u64,
    pub driver_store_oldest_days: Option<u32>,
}

/// A single category estimate used by the doctor pre-flight report.
#[derive(Debug, Clone)]
pub struct CategoryEstimate {
    /// category id, e.g. `user-temp`
    pub id: String,
    /// estimated reclaimable bytes for this category
    pub size_bytes: u64,
    /// risk level (Safe or System) of the category
    pub risk: RiskLevel,
}

/// Pre-flight safety snapshot produced by `sweep doctor`.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    /// reserve file status: ok / missing / consumed
    pub reserve_status: ReserveStatus,
    /// whether sweep is running elevated
    pub elevation: ElevationStatus,
    /// whether toast notifications are available
    pub toast: ToastStatus,
    /// whether guard is installed as a logon task
    pub guard_armed: bool,
    /// per-category estimate of what guard would clean
    pub would_clean: Vec<CategoryEstimate>,
    /// total bytes guard would clean (sum of `would_clean`)
    pub would_clean_total_bytes: u64,
    /// true when the sizing budget expired, making the total a lower bound
    pub would_clean_partial: bool,
    /// current count of detected idle offenders
    pub idle_offender_count: u64,
    /// detected volumes and their media, so the user can see which maintenance
    /// action `sweep optimize` would pick for each drive
    pub volumes: Vec<VolumeInfo>,
}

/// Reserve file status reported by `sweep doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveStatus {
    Ok,
    Missing,
    Consumed,
}

/// Elevation status reported by `sweep doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationStatus {
    Elevated,
    Not,
}

/// Toast availability status reported by `sweep doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastStatus {
    Available,
    Unavailable,
}

/// User-supplied exclusions parsed from `sweep.toml`.
#[derive(Debug, Clone, Default)]
pub struct ExclusionConfig {
    /// absolute/relative directory paths to always skip
    pub paths: Vec<PathBuf>,
    /// category ids to skip entirely (e.g. `dev-pnpm`)
    pub category_ids: Vec<String>,
    /// glob patterns matched (case-insensitively) against candidate paths
    pub globs: Vec<String>,
}

impl ExclusionConfig {
    /// Returns true when no exclusions are configured.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.category_ids.is_empty() && self.globs.is_empty()
    }
}

/// A user-supplied cleaner rule from a TOML `[[category]]` entry.
#[derive(Debug, Clone)]
pub struct RulePackCategory {
    /// unique category id; conflicts with a built-in id are skipped
    pub id: String,
    /// roots to scan for reclaimable bytes
    pub roots: Vec<PathBuf>,
    /// risk level controlling visibility (System hidden unless `--deep`)
    pub risk: RiskLevel,
    /// optional shell command to run instead of trashing (mirrors built-in pnpm)
    pub cleanup_command: Option<String>,
}

/// Append-only journal of trashed items, enabling `sweep undo`.
///
/// Persisted by `infra::undo`; the newest session is the one `sweep undo`
/// restores.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UndoJournal {
    /// one entry per clean session, newest last
    pub sessions: Vec<UndoSession>,
}

/// A single clean session recorded in the undo journal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UndoSession {
    /// unique session identifier
    pub session_id: String,
    /// unix timestamp (seconds) when the session was recorded
    pub timestamp: i64,
    /// per-item original -> trash mappings
    pub items: Vec<UndoItem>,
}

/// A single trashed item recorded for later undo.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UndoItem {
    /// original on-disk location before trashing
    pub original_path: PathBuf,
    /// Match key used to locate the item in the Recycle Bin. Equals the
    /// original path because `trash` 5.x exposes no separate trash identifier.
    pub trash_path: PathBuf,
    /// size of the item when it was trashed (for reporting)
    pub size_bytes: u64,
}

/// A requested process termination, validated against the blocklist + consent.
#[derive(Debug, Clone)]
pub struct KillRequest {
    /// target process id
    pub pid: u32,
    /// target process image name
    pub name: String,
    /// approximate memory/working-set size in bytes (for the consent prompt)
    pub size_bytes: u64,
    /// termination mode: graceful close or forced kill
    pub mode: KillMode,
    /// whether the user has explicitly consented to this request
    pub consent: bool,
}

/// Termination mode for a `KillRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillMode {
    /// graceful close (WM_CLOSE / SIGTERM)
    Close,
    /// forced kill (taskkill /F / SIGKILL)
    Kill,
}

/// Active TUI navigation view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiView {
    /// background process list (RAM / disk writers)
    Background,
    /// idle-offender table
    Idle,
    /// kill confirmation modal
    KillModal,
}

/// Physical media backing a volume.
///
/// Determines which maintenance action is safe: solid-state drives are trimmed,
/// rotational drives are defragmented, and defragmenting an SSD is never
/// correct (it burns write cycles for no seek benefit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// solid-state (SATA SSD, NVMe, or any non-rotational device)
    Ssd,
    /// rotational hard disk
    Hdd,
    /// could not be determined — sweep refuses to guess
    Unknown,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaType::Ssd => write!(f, "ssd"),
            MediaType::Hdd => write!(f, "hdd"),
            MediaType::Unknown => write!(f, "unknown"),
        }
    }
}

/// A volume sweep can report on and maintain.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// mount point / drive root, e.g. `C:\` or `/home`
    pub mount: String,
    /// media backing this volume
    pub media: MediaType,
}

/// The maintenance operation appropriate for a volume's media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceAction {
    /// re-issue TRIM for unused blocks (solid-state only)
    Trim,
    /// defragment (rotational only)
    Defrag,
    /// no safe action; carries the reason to show the user
    Unsupported(String),
}

impl std::fmt::Display for MaintenanceAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaintenanceAction::Trim => write!(f, "trim"),
            MaintenanceAction::Defrag => write!(f, "defrag"),
            MaintenanceAction::Unsupported(_) => write!(f, "none"),
        }
    }
}

/// Result of a `sweep optimize` run for one volume.
#[derive(Debug, Clone)]
pub struct OptimizeOutcome {
    /// the volume that was analyzed or maintained
    pub volume: VolumeInfo,
    /// the action chosen for this volume's media
    pub action: MaintenanceAction,
    /// whether the underlying tool ran without error
    pub succeeded: bool,
    /// whether the disk was actually modified (false for analyze / refusal / failure)
    pub applied: bool,
    /// human-readable detail: tool output summary, or why nothing ran
    pub message: String,
}
