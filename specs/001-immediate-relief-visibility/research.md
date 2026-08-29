# Research: Immediate Relief & Visibility (Stage 1)

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)

## R1: sysinfo 0.39.6 `Process::disk_usage()` API shape

**Decision**: Use `sysinfo 0.39.6` `Process::disk_usage()` returning `DiskUsage { read_bytes, written_bytes, total_read_bytes, total_written_bytes }` — all `u64`.

**Rationale**: Already locked in `Cargo.toml`. The `disk_usage()` method is available after `System::refresh_processes(ProcessesToUpdate::All, true)` which is already called in `SysinfoMonitor::snapshot()`. No additional refresh needed. On Linux, these values come from `/proc/[pid]/io` and require no special privileges for the current process's own data or processes owned by the same user. On Windows, these come from `GetProcessIoCounters`.

**Alternatives considered**:
- `sysinfo::Process::io()` (older API) — removed in 0.39; `disk_usage()` is the replacement
- Manual `/proc/[pid]/io` parsing on Linux — unnecessary since sysinfo abstracts this
- `windows-sys::GetProcessIoCounters` direct FFI — redundant with sysinfo already providing this

## R2: Sparse file creation for reserve.bin

**Decision**: Use `std::fs::File::create` + `file.set_len(512 * 1024 * 1024)` to create a sparse file.

**Rationale**: On NTFS (Windows), `set_len` beyond current EOF creates a sparse file (zero-filled, no actual disk allocation). On ext4/XFS (Linux), same syscall creates a sparse file. The file occupies 0 bytes of actual disk until the OS needs to page it. This is the correct behavior: reserve "exists" logically but doesn't waste space until consumed (at which point it's deleted anyway).

**Alternatives considered**:
- Write 512 MB of zeros — wasteful, defeats purpose
- `fallocate` on Linux — not available in std; would need `nix` crate (new dep, violates frugality)
- Windows `DeviceIoControl` FSCTL_SET_SPARSE — unnecessary; NTFS treats `set_len` as sparse by default

## R3: Disk full error detection (SQLite error 1546)

**Decision**: Match on error string containing "disk I/O error" or "No space left on device" plus SQLite error code 1546 where available. Use `anyhow::Error` downcast to `rusqlite::Error` for code matching, and string matching as fallback.

**Rationale**: SQLite wraps OS errors; the exact error code (1546 = `SQLITE_IOERR`) may not always be exposed through anyhow. String matching on the message is reliable across platforms. The `is_disk_full_error` function should be conservative: false-positive means we consume reserve unnecessarily (safe), false-negative means we fail at 0 B (bad). Err toward consuming.

**Alternatives considered**:
- Check `free_bytes < 0` before open — race condition (disk fills between check and open)
- Only match exact error code 1546 — misses Linux "No space left on device" strings
- Use `std::io::ErrorKind::Other` / `StorageFull` — not reliably propagated through rusqlite

## R4: `free_bytes_on_index_volume()` implementation

**Decision**: Use `sysinfo::Disks` to find the disk whose mount point matches `index_db_path().parent()`, return its `available_space()`. Fall back to 0 if mount point not found.

**Rationale**: `sysinfo::Disks` is already used in `SysinfoMonitor::snapshot()` and refreshed per call. We need a standalone function callable from `main.rs` without a full `SysinfoMonitor` instance. The function uses `Disks::new_with_refreshed_list()` for a one-shot check (low cost, called only before clean/bin operations).

**Alternatives considered**:
- `std::fs::metadata` + `available` — not cross-platform
- `winapi::GetDiskFreeSpaceEx` direct FFI — unnecessary when sysinfo provides this
- Cache disk info in `SysinfoMonitor` — over-engineering for a function called ≤2 times per command

## R5: Idle probe API (Windows)

**Decision**: Use `windows-sys 0.61.2` with `Win32_UI_Input` feature for `GetLastInputInfo` (returns `LASTINPUTINFO { dwTime }` with system tick count) and `GetForegroundWindow` + `GetWindowTextW` for active window title. Compute idle seconds as `(current_tick - last_input_tick) / 1000`.

**Rationale**: Already have `windows-sys` in deps; just need to add `Win32_UI_Input` feature to existing `Cargo.toml` entry. `GetLastInputInfo` is available on all Windows versions, no elevation needed. `GetForegroundWindow` is in `Win32_UI_Windows` which is already a sub-feature of `Win32_UI_Shell` (already enabled) — but may need explicit `Win32_UI_Windows` if not already included.

**Alternatives considered**:
- `GetIdleTime` — not a real API; `GetLastInputInfo` is the standard approach
- `WaitForInputIdle` — works only for process handles, not global idle
- `raw_input_hooks` — too heavy, requires message loop

**Note**: Linux stub returns `IdleInfo { idle_seconds: 0, foreground_title: String::new() }` — heuristic only, no elevation.

## R6: Dev cache paths (cross-platform)

**Decision**: Single `dev_caches.rs` in `infra/` using `std::env::var_os` for platform detection. Paths:

| Tool | Windows | Linux |
|------|---------|-------|
| pnpm | `%LOCALAPPDATA%/pnpm/store` | `~/.local/share/pnpm/store` or `$PNPM_HOME/store` |
| cargo | `~/.cargo/registry/cache`, `~/.cargo/registry/src`, `~/.cargo/git/checkouts` | Same (cross-platform) |
| gradle | `~/.gradle/caches` | Same |
| uv | `~/.local/share/uv` | Same |
| pipx | `~/.local/share/pipx` | Same |

**Rationale**: cargo/gradle/uv/pipx use `~` (home dir) on both OS. pnpm store uses `%LOCALAPPDATA%` on Windows and `~/.local/share` on Linux (or `$PNPM_HOME` if set). All paths are discoverable via `std::env::var_os("HOME")` / `std::env::var_os("LOCALAPPDATA")`. Missing roots are silently skipped (existing pattern in `clean_paths.rs`).

**Alternatives considered**:
- Separate `win/dev_caches.rs` + `linux/dev_caches.rs` — unnecessary duplication; paths are identical except pnpm base
- Shell out to `pnpm store path` — adds process spawn overhead, may not be installed
- Config file for custom paths — Stage 2 scope (cleaner rule packs)

## R7: DiagnoseReport sorting and rollup

**Decision**: `sweep diagnose` sorts rows by `total_bytes` descending, prints `Category | Size | Risk | Reclaim` table, then summary line `potential reclaim: X (Safe Y + ...)`.

**Rationale**: Sorted by size descending puts the biggest wins first (matches user intent: "what's hogging my disk?"). Rollup sums all reclaimable bytes (Risk=Safe categories). "System" risk categories (future Stage 2) will show in table but not contribute to potential reclaim unless `--deep` flag.

**Alternatives considered**:
- Alphabetical sort — less useful for finding hogs
- Risk-first grouping — harder to read for 10+ categories
- JSON output — over-engineering for CLI tool; can add later

## R8: Benchmark before/after measurement

**Decision**: Snapshot free bytes via `Disks::new_with_refreshed_list()` before and after `clean`/`bin --empty` operation. Print `before X free → after Y free (freed Z) in Ns` where `N = elapsed_secs` from `Instant::now()`.

**Rationale**: Uses existing sysinfo `Disks` (no new dep). Measurement is taken around the actual trash/purge operation only (not scan). `Instant::now()` for wall-clock timing. The before/after are on the same volume as `index_db_path()`.

**Alternatives considered**:
- Per-file before/after — too noisy
- Only after total — user can't see delta
- CPU time instead of wall clock — less meaningful for user
