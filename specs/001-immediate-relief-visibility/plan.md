# Implementation Plan: Immediate Relief & Visibility (Stage 1)

**Branch**: `001-immediate-relief-visibility` | **Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-immediate-relief-visibility/spec.md`

## Summary

Stage 1 delivers four fixtures (F1+F5+F2+F3+F7) that prevent sweep from bricking at 0 B, expose dev cache bloat (pnpm/cargo/gradle/uv), add per-process I/O visibility, and provide before/after benchmarks for every clean. The core approach is a 512 MB sparse reserve file consumed automatically when free space is critical, paired with a new `sweep diagnose` command that surfaces reclaimable categories sorted by size.

## Technical Context

**Language/Version**: Rust 2024 edition, stable 1.98+

**Primary Dependencies**: `rusqlite 0.40.2` (bundled SQLite WAL), `sysinfo 0.39.6` (process disk_usage + system snapshot), `windows-sys 0.61.2` (Win32_UI_Input for idle probe), `jwalk 0.9.0` (incremental dir walk), `trash 5.2.6` (recycle bin), `clap 4.6.6` (CLI), `ratatui 0.30.2` + `crossterm 0.29.0` (TUI), `byte-unit 5.2.5` (size formatting), `anyhow 1.0.104` (error handling), `chrono 0.4.45` (timestamps)

**Storage**: SQLite WAL at `%LOCALAPPDATA%\sweep\index.db` (Linux: `~/.local/share/sweep/index.db`); sparse reserve file at `reserve.bin` alongside DB; `SWEEP_DB` env fallback to `D:\sweep\index.db`

**Testing**: `cargo test --locked` on `ubuntu-latest` + `windows-latest` CI; `#[ignore]` for live probes needing real user profile; mock-based unit tests for services/domain

**Target Platform**: Windows 10+ (primary), Linux (equal parity via CI gate)

**Project Type**: Single-binary CLI + TUI tool

**Performance Goals**: `sweep status` < 2s at 0 B (after reserve consume); `sweep diagnose` scan < 5s on typical dev machine; reserve create/consume < 1s

**Constraints**: No elevation, no service stops, no process kills (constitution II-tier 1 only); < 50 MB RSS, ~0% idle CPU; single static binary ~7-8 MB; no new heavy deps

**Scale/Scope**: 15 new/modified FRs; 6 new source files; ~400-500 new LoC; cross-platform (Win+Linux)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Resource Frugality | PASS | Reserve is 512 MB sparse file (zero idle cost when unconsumed); no polling loops added; dir walk reuses existing `jwalk` patterns; sysinfo `disk_usage()` is O(1) per refresh |
| II. Safety-First, Trash-Backed | PASS | All cleaners remain trash-backed (`trash::delete`); `bin --empty` is only permanent delete; no process kills; reserve consumption is idempotent; whitelisted categories only |
| III. Strict Layered Architecture | PASS | New `dev_caches.rs` in `infra/` (shared, no OS imports beyond `std::env`); `diagnose.rs` in `ui/` (clap + formatting only); `SystemSnapshot` extended in `domain/models.rs`; `SysinfoMonitor` disk_usage in `infra/sysinfo_monitor.rs`; `WinIdleProbe` in `infra/win/idle.rs` |
| IV. Test-First & Verification | PASS | Unit tests for reserve lifecycle, empty-only bug fix, dev cache discovery (both OS), diagnose rollup; `#[ignore]` live probes; CI gate maintained |
| V. Cross-Platform Parity | PASS | `dev_caches.rs` is `#[cfg(not(windows))]`-free (shared via `std::env`); idle probe has Linux stub; all paths use platform-conditional `data_dir()` |
| VI. Observability & Trust | PASS | `sweep diagnose` gives pre-flight visibility; benchmark prints before/after free; reserve status surfaced in output; all actions logged |

No violations. No complexity tracking needed.

## Project Structure

### Documentation (this feature)

```text
specs/001-immediate-relief-visibility/
├── plan.md              # This file
├── research.md          # Phase 0: API findings, design decisions
├── data-model.md        # Phase 1: entity definitions + state transitions
├── quickstart.md        # Phase 1: validation guide
├── contracts/           # Phase 1: CLI contract for diagnose + reserve
└── tasks.md             # Phase 2 output (/speckit.tasks - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── domain/
│   ├── models.rs           # MODIFY: add ProcessIoInfo, DiskHeadroom, DiagnoseRow, BenchmarkSample, extend CleanCategory
│   └── traits.rs           # MODIFY: add DiskHeadroomProbe trait
├── infra/
│   ├── paths.rs            # MODIFY: add reserve_path(), ensure_reserve(), consume_reserve(), free_bytes_on_index_volume(), SWEEP_DB fallback
│   ├── dev_caches.rs       # NEW: shared pnpm/cargo/gradle/uv/pipx discovery
│   ├── sysinfo_monitor.rs  # MODIFY: wire Process::disk_usage() into snapshot, implement DiskHeadroomProbe
│   ├── win/
│   │   ├── mod.rs          # MODIFY: add pub mod idle, pub mod dev_caches
│   │   ├── clean_paths.rs  # MODIFY: add dev_caches to discover_categories()
│   │   └── idle.rs         # NEW: WinIdleProbe (GetLastInputInfo + GetForegroundWindow)
│   └── linux/
│       ├── mod.rs          # MODIFY: (no new modules; dev_caches.rs is shared)
│       └── clean_paths.rs  # MODIFY: add dev_caches to discover_categories()
├── services/
│   ├── clean_service.rs    # MODIFY: empty-only bug fix (FR-007), ensure_headroom logic
│   └── diagnose_service.rs # NEW: build DiagnoseReport from categories + snapshot
├── ui/
│   ├── cli.rs              # MODIFY: add Diagnose command variant
│   ├── clean.rs            # MODIFY: benchmark before/after print (FR-013)
│   └── diagnose.rs         # NEW: print Category | Size | Risk | Reclaim table
└── main.rs                 # MODIFY: open_store_with_reserve(), run_status fallback, run_diagnose, run_clean headroom + benchmark, run_bin benchmark, empty-only fix
```

**Structure Decision**: Existing single-project layout preserved. All new files follow established `domain/infra/services/ui` layering. `dev_caches.rs` lives in `infra/` (shared, no OS API imports) and is included by both `win/clean_paths.rs` and `linux/clean_paths.rs`. `idle.rs` is `#[cfg(windows)]`-only with a stub in `linux/`.

## Complexity Tracking

> No violations to justify. All additions fit existing layered architecture.
