# Implementation Plan: Guard Daemon & Deep System Cleaning (Stage 2)

**Branch**: `002-guard-deep-clean` | **Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-guard-deep-clean/spec.md`

## Summary

Stage 2 delivers the guard daemon (auto-rescue background process), deep system cleaning (WU/DO/WinSxS/driver store scan-only by default), service-aware unlock (opt-in service stop for deep clean), idle SSD offender detection, full benchmark logging, and guard autostart. The guard polls every 30s with 3-sample RAM hysteresis and graduated disk rescue (reserve → safe categories → Recycle Bin), all behind a single-instance mutex with 10-minute cooldown. Deep categories are `RiskLevel::System` and hidden unless `--deep` is passed.

## Technical Context

**Language/Version**: Rust 2024 edition, stable 1.98+

**Primary Dependencies**: Existing (`rusqlite 0.40.2`, `sysinfo 0.39.6`, `windows-sys 0.61.2`, `trash 5.2.6`, `clap 4.6.6`, `ratatui 0.30.2`, `byte-unit 5.2.5`, `anyhow 1.0.104`, `chrono 0.4.45`); no new heavy deps (constitution I)

**Storage**: SQLite WAL at `%LOCALAPPDATA%\sweep\index.db`; guard log at `%LOCALAPPDATA%\sweep\guard.log`; guard mutex lock file at `%LOCALAPPDATA%\sweep\guard.lock`

**Testing**: `cargo test --locked` on `ubuntu-latest` + `windows-latest` CI; `#[ignore]` for live Windows-only probes (service stop, dism, toast); mock-based unit tests for services/domain

**Target Platform**: Windows 10+ (primary for deep categories, service lock, toast); Linux (guard polling, idle detection, benchmark work cross-platform; deep system categories skipped)

**Project Type**: Single-binary CLI + TUI tool

**Performance Goals**: Guard poll cycle < 100ms while healthy; deep scan < 10s; idle snapshot diff < 2s; guard < 50 MB RSS, < 1% CPU idle

**Constraints**: No new heavy deps; trash-backed only; guard never auto-kills or auto-stops services (constitution II-tier 1); service stop requires explicit `--stop-services` or `--allow-service-stop`; < 50 MB RSS; single static binary

**Scale/Scope**: ~22 FRs; 6 new source files; ~800-1000 new LoC; cross-platform (Win+Linux for guard/idle/benchmark; Win-only for deep/service lock)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Resource Frugality | PASS | Guard polls 30s with near-zero idle work; 3-sample hysteresis prevents wasted trim attempts; no new heavy deps; file-based mutex is zero-cost when idle; deep scan uses existing dir-walk patterns; idle diff is two sysinfo snapshots (O(n) processes) |
| II. Safety-First, Trash-Backed | PASS | All cleaning remains trash-backed; guard never auto-kills (FR-022); service stop requires explicit flag (FR-014); ServiceGuard uses RAII/drop for guaranteed restore; deep categories hidden by default; cooldown prevents spam |
| III. Strict Layered Architecture | PASS | Guard daemon in `services/guard_service.rs`; deep_clean in `infra/win/deep_clean.rs`; service_lock in `infra/win/service_lock.rs`; idle_service in `services/idle_service.rs`; CLI extensions in `ui/cli.rs`; domain models in `domain/models.rs` |
| IV. Test-First & Verification | PASS | Unit tests for guard loop logic, service_lock RAII, idle snapshot diff, deep scan discovery, benchmark recording; `#[ignore]` for live Windows probes; CI gate maintained |
| V. Cross-Platform Parity | PASS | Guard, idle detection, benchmark work on both OS; deep system categories are `#[cfg(windows)]`-only with graceful no-op on Linux; service_lock is Windows-only with compile gate |
| VI. Observability & Trust | PASS | Guard logs all actions with timestamps; toast notifications for rescues; benchmark before/after on every clean; diagnose --deep shows Safe/System split; idle table shows reason codes |

No violations. No complexity tracking needed.

## Project Structure

### Documentation (this feature)

```text
specs/002-guard-deep-clean/
├── plan.md              # This file
├── research.md          # Phase 0: Windows API findings, design decisions
├── data-model.md        # Phase 1: entity definitions + state transitions
├── quickstart.md        # Phase 1: validation guide
├── contracts/           # Phase 1: CLI contracts for guard, idle, diagnose --deep
└── tasks.md             # Phase 2 output (/speckit.tasks - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── domain/
│   ├── models.rs           # MODIFY: add risk field to CleanCategory, GuardConfig, RamSnapshot, DiskSnapshot, ServiceGuardState, IdleSsdOffender, GuardBenchmark, GuardLogEntry
│   └── traits.rs           # MODIFY: add GuardMonitor trait (snapshot RAM/disk)
├── infra/
│   ├── paths.rs            # MODIFY: add guard_log_path(), guard_lock_path()
│   ├── schedule.rs         # MODIFY: add guard_install/guard_remove (schtasks /SC ONLOGON)
│   ├── sysinfo_monitor.rs  # MODIFY: implement GuardMonitor for SysinfoMonitor
│   └── win/
│       ├── mod.rs          # MODIFY: add pub mod deep_clean, pub mod service_lock
│       ├── clean_paths.rs  # MODIFY: add deep system categories (WU, DO, drivers) behind risk=System
│       ├── deep_clean.rs   # NEW: WU/DO/WinSxS/driver discovery + dism analysis (read-only)
│       └── service_lock.rs # NEW: ServiceGuard RAII wrapper (wuauserv, bits, dosvc)
├── services/
│   ├── guard_service.rs    # NEW: guard daemon loop, 3-sample hysteresis, disk rescue, cooldown, logging, toast
│   ├── idle_service.rs     # NEW: I/O snapshot diff, idle offender detection
│   └── benchmark.rs        # NEW: before/after free + per-category removed_bytes recording
├── ui/
│   ├── cli.rs              # MODIFY: add Guard command, Idle command, extend Clean/Diagnose with --deep/--stop-services
│   ├── guard.rs            # NEW: guard CLI output (toast, log formatting)
│   └── idle.rs             # NEW: idle offender table formatting
└── main.rs                 # MODIFY: run_guard, run_idle, extend run_clean/run_diagnose with --deep/--stop-services
```

**Structure Decision**: Existing single-project layout preserved. All new files follow `domain/infra/services/ui` layering. `deep_clean.rs` and `service_lock.rs` are `#[cfg(windows)]`-only. `guard_service.rs` and `idle_service.rs` are cross-platform with platform-specific behavior behind `#[cfg]` in the infra layer.

## Complexity Tracking

> No violations to justify. All additions fit existing layered architecture.
