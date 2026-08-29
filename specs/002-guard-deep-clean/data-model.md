# Data Model: Guard Daemon & Deep System Cleaning (Stage 2)

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Existing Entities (modified)

### CleanCategory — add risk field

**File**: `src/domain/models.rs`

```rust
pub struct CleanCategory {
    pub id: String,
    pub title: String,
    pub roots: Vec<PathBuf>,
    pub risk: RiskLevel,  // NEW — defaults to Safe for all existing categories
}
```

**Migration**: All existing `discover_categories()` calls in `win/clean_paths.rs` and `linux/clean_paths.rs` add `risk: RiskLevel::Safe`. New deep categories add `risk: RiskLevel::System`.

### BenchmarkSample — add per-category breakdown

**File**: `src/domain/models.rs`

```rust
pub struct BenchmarkSample {
    pub before_free_bytes: u64,
    pub after_free_bytes: u64,
    pub elapsed_secs: f64,
    pub category_bytes: Vec<(String, u64)>,  // NEW — (category_id, bytes_removed)
}
```

**Usage**: Every clean/guard operation records per-category contributions. `freed_bytes()` remains the total. `safe_freed()` and `system_freed()` filter by category risk level (requires category risk lookup).

### DiagnoseReport — extend with System total

**File**: `src/domain/models.rs`

```rust
pub struct DiagnoseReport {
    pub rows: Vec<DiagnoseRow>,
    pub total_reclaimable: u64,
    pub safe_reclaimable: u64,    // NEW — sum where risk == Safe
    pub system_reclaimable: u64,  // NEW — sum where risk == System
    pub idle: Option<IdleProbeResult>,
}
```

### TrimOutcome — unchanged

Already tracks `attempted_pids`, `succeeded`, `failed`, `standby_attempted`, `standby_ok`. Guard reuses this for RAM trim results.

## New Entities

### GuardConfig

**File**: `src/domain/models.rs`

```rust
pub struct GuardConfig {
    pub ram_threshold_pct: u8,      // default 90
    pub disk_min_gb: u64,           // default 2
    pub interval_secs: u64,         // default 30
    pub once: bool,                 // default false
    pub allow_service_stop: bool,   // default false
    pub allow_kill: bool,           // default false
    pub cooldown_secs: u64,         // default 600 (10 min)
}
```

**Source**: Parsed from CLI args in `ui/cli.rs`. Passed to `GuardService::run()`.

### RamSnapshot

**File**: `src/domain/models.rs`

```rust
pub struct RamSnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub standby_bytes: u64,  // Windows-only, 0 on Linux
}
```

**Source**: `SysinfoMonitor` (existing) + Windows standby query (existing in `win/ram.rs`).

### DiskSnapshot

**File**: `src/domain/models.rs`

```rust
pub struct DiskSnapshot {
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub mount_point: PathBuf,
}
```

**Source**: `SysinfoMonitor` or `free_bytes_on_index_volume()` (existing in `paths.rs`).

### ServiceGuardState

**File**: `src/domain/models.rs`

```rust
pub struct ServiceGuardState {
    pub services: Vec<ServiceEntry>,
}

pub struct ServiceEntry {
    pub name: String,
    pub was_running: bool,
}
```

**Source**: Created by `ServiceGuard::new()` in `infra/win/service_lock.rs`. Consumed on `Drop` to restore services.

### IdleSsdOffender

**File**: `src/domain/models.rs`

```rust
pub struct IdleSsdOffender {
    pub pid: u32,
    pub name: String,
    pub idle_mins: u64,
    pub write_per_hour_mb: f64,
    pub ram_bytes: u64,
    pub reason: IdleReason,
}

pub enum IdleReason {
    CacheBloat,
    LogSpam,
    Unknown,
}
```

**Source**: Computed by `IdleService::detect()` in `services/idle_service.rs` from two sysinfo snapshots.

### GuardBenchmark

**File**: `src/domain/models.rs`

```rust
pub struct GuardBenchmark {
    pub action: GuardAction,
    pub benchmark: BenchmarkSample,
    pub timestamp: String,  // ISO8601
}

pub enum GuardAction {
    RamTrim { pids_trimmed: u32 },
    DiskRescue { phase: DiskRescuePhase },
    RecycleBinPurge,
}

pub enum DiskRescuePhase {
    ReserveConsumed,
    SafeCategories,
    RecycleBinPurge,
}
```

**Source**: Assembled by `GuardService` after each rescue action. Written to `guard.log`.

### GuardLogEntry

**File**: `src/domain/models.rs`

```rust
pub struct GuardLogEntry {
    pub timestamp: String,
    pub level: String,       // "INFO", "WARN", "ACTION"
    pub message: String,
    pub bytes_freed: Option<u64>,
    pub details: Option<String>,
}
```

**Source**: Written by `GuardService` to `%LOCALAPPDATA%\sweep\guard.log`. Parsed by `guard --status` for last-action display.

### DeepScanResult

**File**: `src/domain/models.rs`

```rust
pub struct DeepScanResult {
    pub wu_download_bytes: u64,
    pub delivery_optimization_bytes: u64,
    pub winsxs_reclaimable_bytes: Option<u64>,  // None if dism unavailable
    pub driver_store_bytes: u64,
    pub driver_store_oldest_days: u32,
}
```

**Source**: `DeepCleanService::scan()` in `infra/win/deep_clean.rs`. Used by `diagnose --deep` and `clean --deep`.

## State Transitions

### Guard Daemon Lifecycle

```
[NOT RUNNING] --sweep guard--> [STARTING]
  acquire mutex lock file
  if lock fails → exit "guard already running"
  if --once → run one poll cycle → exit
  else → [POLLING] loop every interval_secs
    on RAM pressure (3 consecutive ≥ threshold) → [TRIMMING]
      RamService::optimize(top10) + purge_standby
      log + toast → cooldown → back to [POLLING]
    on disk pressure (free < min_gb) → [DISK_RESCUE]
      consume_reserve()
      trash safe categories
      if still low → TrashBin::purge_all()
      log + toast → cooldown → back to [POLLING]
    on no pressure → [POLLING] (near-zero work)
  on signal/ctrl-c → release mutex → [NOT RUNNING]
```

### Service Lock Lifecycle

```
[CLEANING] --stop-services flag--> [SERVICES_STOPPING]
  for each service in [wuauserv, bits, dosvc]:
    OpenSCManager → OpenService → ControlService(STOP)
    record was_running = true
  → [SERVICES_STOPPED]
  clean SoftwareDistribution\Download
  → [SERVICES_RESTORING]
  for each was_running=true service:
    OpenService → StartService
  → [CLEANING_COMPLETE]
  Drop(ServiceGuard) → ensure restore even on panic
```

### Idle Detection Lifecycle

```
[sweep idle] → [SNAPSHOT_1]
  record all process I/O counters + foreground PID
  sleep 60s → [SNAPSHOT_2]
  for each process in snapshot_1 ∩ snapshot_2:
    write_delta = snap2.write_bytes - snap1.write_bytes
    idle_secs = now - process.last_activity (or start_time proxy)
    if idle_secs > threshold && write_delta/hour > threshold && pid != foreground:
      add to offenders list
  → [DISPLAY_TABLE]
  sort by write_per_hour descending, limit to --top N
```

### Deep Scan Decision Tree

```
sweep diagnose (no --deep) → Safe categories only (existing behavior)
sweep diagnose --deep → Safe + System categories
  discover WU download size
  discover DO cache size
  run dism /AnalyzeComponentStore (if available)
  discover driver store size + oldest age
  → DiagnoseReport with safe_reclaimable + system_reclaimable

sweep clean (no --deep) → Safe categories only
sweep clean --deep → Safe + System categories
  without --stop-services → skip WU download (locked by wuauserv)
  with --stop-services → stop wuauserv+bits → trash WU download → restore
```

## Validation Rules

- `GuardConfig.ram_threshold_pct` must be in 1..=100; default 90
- `GuardConfig.disk_min_gb` must be >= 1; default 2
- `GuardConfig.interval_secs` must be >= 5; default 30
- `GuardConfig.cooldown_secs` must be >= 60; default 600
- `RamSnapshot.standby_bytes` must be 0 on non-Windows platforms
- `IdleSsdOffender.write_per_hour_mb` must be > 0.0
- `ServiceGuardState.services` must not be empty when constructed
- `BenchmarkSample.category_bytes` must sum to `freed_bytes()` within rounding tolerance
- Guard mutex file must be released on all exit paths (including panic)
- `DeepScanResult.winsxs_reclaimable_bytes` is `None` when dism is unavailable or requires elevation
