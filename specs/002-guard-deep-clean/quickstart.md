# Quickstart Validation Guide: Stage 2

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Prerequisites

- Rust 1.98+ stable installed (`rustc --version`)
- `cargo test --locked` passes on current branch before changes
- Windows 10/11 (for deep categories, service lock, toast); Linux for guard/idle/benchmark
- `%LOCALAPPDATA%\sweep\` directory writable
- Elevated prompt for service stop/start tests (V5, V6)

## Validation Scenarios

### V1: Guard single-pass trim (FR-001, FR-002, FR-003, FR-008)

**Setup**: None (uses existing system state)

```
sweep guard --once
# EXPECT: poll cycle runs, reports RAM/disk status
# EXPECT: if RAM ≥90% for 3 samples → trim top-10, purge standby
# EXPECT: guard.log entry with timestamp, action, bytes freed
# EXPECT: toast notification (if Windows 10+ with PowerShell)
```

**Unit test** (`cargo test guard_once_trim`):
- Mock `GuardMonitor` to return RAM ≥90% for 3 samples
- Assert trim is triggered, `TrimOutcome` has `succeeded > 0`

### V2: Guard cooldown (FR-005)

```
sweep guard --once
# (force RAM pressure in test)
# EXPECT: action performed
# run again immediately
# EXPECT: "cooldown active, skipping" in log
```

**Unit test** (`cargo test guard_cooldown`):
- Simulate rescue action at T=0
- Assert next poll within 600s skips action

### V3: Guard single-instance mutex (FR-009)

```
# Terminal 1:
sweep guard --once
# Terminal 2 (while guard is running):
sweep guard
# EXPECT: "guard is already running" + exit code 1
```

### V4: Guard disk rescue graduated response (FR-004)

```
sweep guard --once --disk-min-gb 999
# EXPECT: disk pressure detected
# EXPECT: Phase 1: consume reserve (freed ~512 MB)
# EXPECT: Phase 2: trash safe categories
# EXPECT: Phase 3: purge Recycle Bin (if still below threshold)
# EXPECT: guard.log with phase-by-phase entries
# EXPECT: benchmark: before → after free space
```

### V5: Service lock RAII (FR-013, FR-014)

**Setup**: Elevated prompt required

```
# Unit test (no elevation needed):
cargo test service_guard_drop -- --nocapture
# EXPECT: services stopped on creation, restored on drop
# EXPECT: even if cleanup code panics, Drop restores services

# Integration test (elevated):
sweep clean --deep --stop-services -y --only wu-downloads
# EXPECT: wuauserv and bits stopped before clean
# EXPECT: wuauserv and bits restored after clean (check: sc query wuauserv)
# EXPECT: SoftwareDistribution\Download files moved to recycle bin
```

### V6: Deep scan diagnose (FR-010, FR-011, FR-012)

```
sweep diagnose
# EXPECT: only Safe categories shown (no WU/DO/WinSxS/drivers)

sweep diagnose --deep
# EXPECT: Safe + System categories shown
# EXPECT: wu-downloads with risk=System
# EXPECT: delivery-optimization with risk=System
# EXPECT: winsxs-reclaimable with risk=System (or "unavailable" if dism fails)
# EXPECT: driver-store with risk=System
# EXPECT: summary line: "Safe X, System Y"
```

**Unit test** (`cargo test deep_scan_discover`):
- Mock filesystem paths for WU/DO/driver store
- Assert `DeepScanResult` fields populated correctly
- Assert dism parse test with sample output

### V7: Clean --deep --scan-only (FR-021)

```
sweep clean --deep --scan-only
# EXPECT: all categories (Safe + System) listed
# EXPECT: NO files deleted
# EXPECT: output matches diagnose --deep format
```

### V8: Clean --deep without --stop-services

```
sweep clean --deep -y
# EXPECT: WU downloads listed but shows "(locked)" in RECLAIM
# EXPECT: safe categories cleaned
# EXPECT: services NOT stopped
```

### V9: Idle SSD detection (FR-015, FR-016)

**Setup**: Start a background process that writes >100 MB/h while idle

```
sweep idle
# EXPECT: table with PID | APP | IDLE | WRITE/h | RAM | REASON
# EXPECT: foreground app excluded
# EXPECT: empty state if no offenders

sweep idle --top 5 --idle-mins 60 --min-write-mb 200
# EXPECT: filtered results matching thresholds
```

**Unit test** (`cargo test idle_snapshot_diff`):
- Mock two sysinfo snapshots with known write deltas
- Assert offenders detected correctly
- Assert foreground PID excluded

### V10: Idle cache clean (FR-017)

```
sweep idle --clean-cache
# EXPECT: whitelisted cache cleaned for flagged processes
# EXPECT: bytes freed per process reported
```

### V11: Benchmark logging (FR-018, FR-019)

```
sweep clean --only npm-cache -y
# EXPECT: "before X free → after Y free (freed Z) in Ns"
# EXPECT: per-category breakdown below benchmark line

sweep guard --once --disk-min-gb 999
# EXPECT: guard.log contains benchmark entry with before/after

sweep diagnose --deep
# EXPECT: summary shows "Safe X, System Y" split
```

### V12: Guard autostart (FR-020)

```
sweep schedule --guard-install
# EXPECT: "scheduled task 'SweepGuard' installed (on logon)"

sweep schedule --guard-status
# EXPECT: "guard autostart: installed"

sweep schedule --guard-remove
# EXPECT: "scheduled task 'SweepGuard' removed"
```

### V13: Cross-platform CI gate

```
cargo test --locked
# EXPECT: all tests pass (0 failures, #[ignore] tests allowed)
cargo build --release
# EXPECT: binary builds successfully
```

## Expected Test Counts

After implementation, `cargo test --locked` should report:
- Existing tests: all passing (no regressions)
- New unit tests: ~15-20 (guard loop, cooldown, hysteresis, service lock RAII, idle snapshot diff, deep scan discovery, benchmark recording, toast, mutex)
- New integration tests: ~4-6 (guard --once end-to-end, deep clean with service stop, idle detection, benchmark output)
- Total new LoC: ~800-1000 across 6 new files + modifications to 8 existing files
