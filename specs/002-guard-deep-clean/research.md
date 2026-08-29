# Research: Guard Daemon & Deep System Cleaning (Stage 2)

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## R1: Windows Service Control Manager via `windows-sys`

**Decision**: Use `windows-sys` crate FFI (`OpenSCManagerW`, `OpenServiceW`, `ControlService`, `StartServiceW`, `CloseServiceHandle`) for service stop/start. No new dependency — `windows-sys 0.61.2` already in `Cargo.toml`.

**Rationale**: `windows-sys` is the project's declared FFI approach (constitution I, "Prefer declaration-only FFI"). SCM operations are simple: open manager → open service by name → control-service STOP → wait → start-service on restore. Error handling: log and continue if stop fails (constitution II, graceful degradation).

**Alternatives considered**:
- `service-manager` crate: adds a dependency for trivial SCM calls — rejected per constitution I
- `powershell Stop-Service`: adds process spawn overhead and parse fragility — rejected
- Manual `sc.exe` invocation: same overhead as PowerShell — rejected

**Key API patterns**:
```rust
// Stop: OpenSCManagerW → OpenServiceW(name) → ControlService(SERVICE_CONTROL_STOP) → wait
// Start: OpenServiceW(name) → StartServiceW(0, null) → CloseServiceHandle
// RAII: Drop impl calls StartServiceW for each stopped service
```

**Risk**: Some services (wuauserv) may take time to stop. Timeout: 30 seconds, then log warning and continue.

## R2: DISM Component Store Analysis

**Decision**: Parse output of `dism /Online /Cleanup-Image /AnalyzeComponentStore` for WinSxS reclaimable estimate. Never invoke cleanup — read-only analysis only.

**Rationale**: DISM is the official Microsoft tool for WinSxS analysis. The `/AnalyzeComponentStore` flag returns a line like "Total Windows Store Package Size: X" and "Estimated Component Store Cleanup Needed: Y". Parsing is straightforward with line-matching.

**Alternatives considered**:
- Direct WinSxS directory walk: extremely slow (100k+ hardlinks), inaccurate — rejected
- `CleanMgr` API: deprecated, no programmatic interface — rejected
- WMI `Win32_TaskScheduler`: doesn't expose component store data — rejected

**Key considerations**:
- Requires elevation on some systems; graceful "access denied" fallback
- Output encoding may vary (UTF-16 on some locales); use `OsString` → lossy conversion
- `#[cfg(windows)]`-only; Linux gets no-op deep categories

## R3: Windows Toast Notifications via PowerShell

**Decision**: Use `powershell -Command "[Windows.UI.Notifications.ToastNotificationManager, ...]"` one-liner for toast. This is the existing pattern from ROADMAP.md:30.

**Rationale**: Lightweight, no new deps, works on Windows 10+. Graceful no-op if PowerShell or WinRT unavailable (logged to guard.log).

**Alternatives considered**:
- `winrt-notification` crate: adds dependency — rejected per constitution I
- `notify-rust`: cross-platform but heavier — rejected
- `tray-icon`: requires a running window — rejected

## R4: File-Based Single-Instance Mutex

**Decision**: Create `%LOCALAPPDATA%\sweep\guard.lock` with exclusive file open (`OpenOptions::new().create(true).write(true).exclusive(true)`). If lock fails, another guard instance is running — exit with message.

**Rationale**: Simple, zero-dep, works across Rust versions. File is created on guard start, locked for duration, released on exit (even on panic via `Drop` for `File`).

**Alternatives considered**:
- Named pipe: more complex, no benefit over file lock — rejected
- PID file: race conditions on crash recovery — rejected
- Registry key: requires elevation on some configs — rejected

## R5: Guard Log Format and Rotation

**Decision**: Append-only text log at `%LOCALAPPDATA%\sweep\guard.log`. Each line: `[ISO8601 timestamp] [ACTION] [details] [bytes_freed]`. No rotation in Stage 2 — log is small (one line per 10-min cooldown cycle). Future: size-based rotation.

**Rationale**: Simple, debuggable, grep-friendly. Guard runs infrequently enough that log growth is negligible (< 1 KB/day typical).

## R6: Idle SSD Snapshot Diff Algorithm

**Decision**: Take two `sysinfo::System` snapshots 60 seconds apart. For each process: compute `write_delta = snapshot2.write_bytes - snapshot1.write_bytes`, `idle_secs` from process start time vs now (or use last activity time if available). Filter: `idle_secs > threshold && write_delta/hour > threshold && pid != foreground_pid`.

**Rationale**: `sysinfo` crate already provides `Process::disk_usage()` with cumulative `written_bytes`. Delta between two snapshots gives write rate. Foreground detection via `infra/win/idle.rs` (existing `GetForegroundWindow`).

**Alternatives considered**:
- ETW tracing: powerful but requires elevation and complex setup — rejected for Stage 2
- Performance Monitor counters: system-wide, not per-process — rejected
- `/proc/[pid]/io` on Linux: already available via sysinfo — cross-platform works

## R7: Deep Clean Category Risk Classification

**Decision**: Extend `CleanCategory` with a `risk: RiskLevel` field. `RiskLevel` enum already exists in `domain/models.rs` (Safe, System). All existing categories default to `Safe`. New WU/DO/WinSxS/driver categories get `System`. Guard and default `sweep clean` only process `Safe` categories; `--deep` flag unlocks `System`.

**Rationale**: Fits existing `RiskLevel` enum (already used by `DiagnoseRow`). Filter at the UI/service layer rather than compile-time — keeps discovery code always available for `diagnose --deep`.

## R8: Guard Autostart via schtasks

**Decision**: Extend `schedule.rs` with a second task name `"SweepGuard"` using `/SC ONLOGON /TR "sweep guard"`. Separate from the existing `"SweepIndex"` daily task.

**Rationale**: Reuses existing `schtasks` infrastructure in `schedule.rs`. ONLOGON trigger ensures guard starts when user logs in, not on a schedule. The `--install` / `--remove` / `--status` flags on `sweep schedule` need to support both tasks (or add `--guard` flag).

## R9: Benchmark Recording Per Category

**Decision**: Add `category_bytes: Vec<(String, u64)>` to `BenchmarkSample`. Each clean/guard operation records which categories contributed how many bytes. `GuardBenchmark` wraps this with guard-specific metadata (action type, cooldown status).

**Rationale**: Extends existing `BenchmarkSample` (already has `before_free_bytes`, `after_free_bytes`, `elapsed_secs`). Per-category breakdown enables `diagnose --deep` to show "Safe X + System Y" split.

## R10: Service Guard Drop Semantics

**Decision**: `ServiceGuard` struct holds `Vec<(String, WasRunning)>`. On creation: stop each service, record whether it was running. On `Drop`: for each service that was running, attempt start. If start fails, log warning. Never panic in Drop.

**Rationale**: RAII ensures services are always restored, even on panic. The `Drop` impl catches start failures gracefully (constitution II — never lose data/capability silently). The struct is `pub` in `infra/win/service_lock.rs` and used by `services/clean_service.rs` when `--stop-services` is active.
