# Data Model: Immediate Relief & Visibility (Stage 1)

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)

## Existing Entities (modified)

### ProcessMemInfo → extended with I/O fields

**File**: `src/domain/models.rs`

```rust
pub struct ProcessMemInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    // NEW fields (FR-010):
    pub read_bytes: u64,        // bytes read from disk (process lifetime)
    pub write_bytes: u64,       // bytes written to disk (process lifetime)
    pub total_written_bytes: u64, // total bytes written (all time)
}
```

**Populated by**: `SysinfoMonitor::snapshot()` calls `process.disk_usage()` after `refresh_processes()`. Default to 0 if unavailable (Linux non-root, or sysinfo returns zeros).

### CleanCategory → unchanged for Stage 1

Existing fields `id`, `title`, `roots` are sufficient. Dev cache categories reuse the same struct with new `id` values (`pnpm`, `cargo-cache`, `gradle-cache`, `uv-cache`, `pipx-cache`).

### SystemSnapshot → unchanged

Already contains `memory`, `disks`, `top_processes`. `ProcessMemInfo` extension (above) provides I/O data within existing structure.

### CategoryScan → unchanged

Already contains `category_id`, `title`, `items`, `total_bytes`, `files`. Used by both `sweep clean --scan-only` and `sweep diagnose`.

## New Entities

### DiskHeadroom

**File**: `src/domain/models.rs` (new struct)

```rust
pub struct DiskHeadroom {
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub mount_point: std::path::PathBuf,
}
```

**Source**: `SysinfoMonitor` (or standalone `free_bytes_on_index_volume()` in `paths.rs`).

**Usage**: Checked before clean/bin operations. Threshold `<256 MB` triggers reserve consumption.

### IdleProbeResult

**File**: `src/domain/models.rs` (new struct)

```rust
pub struct IdleProbeResult {
    pub idle_seconds: u64,
    pub foreground_title: String,
}
```

**Source**: `WinIdleProbe` on Windows (GetLastInputInfo + GetForegroundWindow); stub `{ 0, "" }` on Linux.

**Usage**: Read-only diagnostic in `sweep diagnose` output; no auto-action in Stage 1.

### DiagnoseRow

**File**: `src/domain/models.rs` (new struct)

```rust
pub struct DiagnoseRow {
    pub category_id: String,
    pub title: String,
    pub size_bytes: u64,
    pub risk: RiskLevel,
    pub reclaimable: bool,
}

pub enum RiskLevel {
    Safe,
    System,
}
```

**Source**: Built by `DiagnoseService` from `CategoryScan` + `RiskLevel` mapping. All Stage 1 dev categories are `Safe`.

### DiagnoseReport

**File**: `src/domain/models.rs` (new struct)

```rust
pub struct DiagnoseReport {
    pub rows: Vec<DiagnoseRow>,
    pub total_reclaimable: u64,
    pub idle: Option<IdleProbeResult>,
}
```

**Source**: Aggregated by `DiagnoseService`.

**Sorted by**: `size_bytes` descending.

### BenchmarkSample

**File**: `src/domain/models.rs` (new struct)

```rust
pub struct BenchmarkSample {
    pub before_free_bytes: u64,
    pub after_free_bytes: u64,
    pub elapsed_secs: f64,
}

impl BenchmarkSample {
    pub fn freed_bytes(&self) -> u64 {
        self.after_free_bytes.saturating_sub(self.before_free_bytes)
    }
}
```

**Source**: Captured by wrapping clean/bin operations in `main.rs`. Before snapshot taken right before trash/purge; after snapshot taken immediately after.

## State Transitions

### Reserve File Lifecycle

```
[NOT EXISTS] --ensure_reserve()--> [EXISTS, 512 MB sparse]
[EXISTS] --consume_reserve()--> [NOT EXISTS] (returns freed_bytes)
[NOT EXISTS] after clean/bin --empty--ensure_reserve()--> [EXISTS, 512 MB sparse]
[EXISTS, locked by AV] --consume_reserve()--> [EXISTS] (returns None, logged)
```

**Guards**:
- `ensure_reserve()` is idempotent: if file exists with correct size, no-op
- `consume_reserve()` is idempotent: if file missing, returns `None`
- Re-creation after clean/bin is conditional on `free_bytes >= 1 GB`

### Disk-Headroom Decision Tree

```
free_bytes >= 256 MB  → proceed with operation normally
free_bytes < 256 MB   → consume_reserve()
                         reserve consumed → retry operation
                         reserve missing   → fail with actionable hint
free_bytes = 0 (error) → is_disk_full_error()
                          consume_reserve() → retry open
                          retry fails       → fallback (status prints RAM/disks only)
```

### Clean --only Filter Fix

```
only.is_empty() == true   → treat as None (clean all)
only.is_empty() == false  → filter by category IDs (existing behavior)
```

## Validation Rules

- Reserve size must be exactly `512 * 1024 * 1024` bytes (±1 MB tolerance for filesystem rounding)
- `DiskHeadroom.free_bytes` must be `u64::MAX` sentinel or 0 if measurement unavailable (never panic)
- `DiagnoseReport.rows` must be sorted by `size_bytes` descending; ties broken by `category_id` lexicographic
- `BenchmarkSample.elapsed_secs` must be `>= 0.0`; negative values clamped to 0.0
- All `RiskLevel::Safe` categories contribute to `total_reclaimable`; `RiskLevel::System` does not (Stage 1)
