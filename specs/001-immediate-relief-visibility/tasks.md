# Tasks: Immediate Relief & Visibility (Stage 1)

**Input**: Design documents from `/specs/001-immediate-relief-visibility/`

**Prerequisites**: plan.md, spec.md, data-model.md, contracts/, research.md, quickstart.md

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Exact file paths provided for every task

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Extend domain models and add reserve file helpers that ALL user stories depend on.

- [X] T001 [P] Extend `ProcessMemInfo` with I/O fields (`read_bytes`, `write_bytes`, `total_written_bytes`) in `src/domain/models.rs`
- [X] T002 [P] Add new domain structs `DiskHeadroom`, `IdleProbeResult`, `DiagnoseRow`, `RiskLevel`, `DiagnoseReport`, `BenchmarkSample` in `src/domain/models.rs`
- [X] T003 Add `reserve_path()`, `ensure_reserve()`, `consume_reserve()`, `is_disk_full_error()`, `free_bytes_on_index_volume()` functions in `src/infra/paths.rs` — includes `SWEEP_DB` env fallback to `D:` in `index_db_path()`
- [X] T004 [P] Add `Win32_UI_Input` feature to `windows-sys` dependency in `Cargo.toml`

**Checkpoint**: Domain models extended, reserve file helpers available, windows-sys ready for idle probe.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Wire sysinfo disk_usage into SysinfoMonitor and register new module declarations. MUST complete before any user story.

- [X] T005 Wire `Process::disk_usage()` into `SysinfoMonitor::snapshot()` to populate `read_bytes`/`write_bytes`/`total_written_bytes` on `ProcessMemInfo` in `src/infra/sysinfo_monitor.rs`
- [X] T006 Add `pub mod dev_caches;` to `src/infra/mod.rs` and add `pub mod idle;` to `src/infra/win/mod.rs`
- [X] T007 Fix empty-only bug: change `CleanService::run()` to treat `Some(&[])` as `None` (clean all) in `src/services/clean_service.rs`
- [X] T008 Add `sweep diagnose` variant to `Command` enum in `src/ui/cli.rs` and wire `run_diagnose` dispatch in `src/main.rs`

**Checkpoint**: SysinfoMonitor returns I/O data, empty-only bug fixed, CLI has diagnose command registered.

---

## Phase 3: User Story 1 — Survive 0 B Disk Full (Priority: P1) 🎯 MVP

**Goal**: Never get bricked at 0 B again. `sweep status` shows RAM/disks/top even when index DB can't open. Reserve consumed automatically, re-created after clean.

**Independent Test**: Mock disk-full, verify status prints fallback. Verify reserve lifecycle (create → consume → re-create).

### Implementation for User Story 1

- [X] T009 [US1] Implement `open_store_with_reserve()` in `src/main.rs` — on disk-full error, consume reserve and retry once; on second failure, return `None` instead of bailing
- [X] T010 [US1] Modify `run_status` in `src/main.rs` to handle `open_store_with_reserve()` returning `None` — print RAM/disks/top-processes via `SysinfoMonitor` with notice `index: unavailable (disk full, reserve consumed — run sweep bin --empty)`
- [X] T011 [US1] Add `ensure_reserve()` call at start of `run_status` and `run_index` in `src/main.rs`
- [X] T012 [US1] Add headroom check in `run_clean`: if `free_bytes_on_index_volume() < 256 MB`, call `consume_reserve()` before `svc.run()` in `src/main.rs` (both `#[cfg(windows)]` and `#[cfg(not(windows))]` variants)
- [X] T013 [US1] Add headroom check + `consume_reserve()` before `bin.purge_all()` in `run_bin` in `src/main.rs`
- [X] T014 [US1] Add `ensure_reserve()` re-creation after successful clean and bin --empty when `free_bytes >= 1 GB` in `src/main.rs`
- [X] T015 [US1] Add `SWEEP_DB` env var check in `index_db_path()` in `src/infra/paths.rs` — if set, use that path instead of default `data_dir().join("index.db")`
- [X] T016 [US1] Add unit tests for reserve lifecycle (ensure/consume/idempotent/missing) in `src/infra/paths.rs`
- [X] T017 [US1] Add unit test for status fallback at 0 B (mock disk-full error → verify fallback message) in `src/main.rs` or integration test

**Checkpoint**: `sweep status` works at 0 B. Reserve created on first run, consumed when low, re-created after clean. `SWEEP_DB` env honored. Empty-only bug fixed (T007).

---

## Phase 4: User Story 2 — Dev Cache Bloat Reclaim (Priority: P1)

**Goal**: Discover pnpm/cargo/gradle/uv/pipx caches and include them in clean scans on both Windows and Linux.

**Independent Test**: Seed cache dirs, run `sweep clean --scan-only`, verify dev categories appear with non-zero sizes.

### Implementation for User Story 2

- [X] T018 [P] [US2] Create `src/infra/dev_caches.rs` — shared cross-platform module with `discover_dev_categories() -> Vec<CleanCategory>` that finds pnpm (`%LOCALAPPDATA%/pnpm/store` on Win, `~/.local/share/pnpm/store` on Linux or `$PNPM_HOME`), cargo (`~/.cargo/registry/cache`, `~/.cargo/registry/src`, `~/.cargo/git/checkouts`), gradle (`~/.gradle/caches`), uv (`~/.local/share/uv`), pipx (`~/.local/share/pipx`); skip missing roots silently
- [X] T019 [US2] Add dev categories from `crate::infra::dev_caches::discover_dev_categories()` into `discover_categories()` in `src/infra/win/clean_paths.rs`
- [X] T020 [US2] Add dev categories from `crate::infra::dev_caches::discover_dev_categories()` into `discover_categories()` in `src/infra/linux/clean_paths.rs`
- [X] T021 [US2] Add unit test for dev cache discovery in `src/infra/dev_caches.rs` — create temp dirs for each tool, verify categories returned with correct IDs, verify missing roots skipped
- [X] T022 [US2] Add integration test for `discover_categories()` including dev caches in `src/infra/linux/clean_paths.rs` (analogous to existing `builds_only_existing_categories` test at line 92)

**Checkpoint**: `sweep clean --scan-only` shows pnpm/cargo/gradle/uv sizes on both OS. `sweep clean --only cargo -y` trashes only cargo paths.

---

## Phase 5: User Story 3 — Hogging Visibility (Priority: P2)

**Goal**: `sweep diagnose` prints sorted table `Category | Size | Risk | Reclaim` with potential reclaim rollup. Per-process I/O visible. Idle probe skeleton on Windows.

**Independent Test**: Run `sweep diagnose`, verify sorted table output with rollup. Verify SystemSnapshot has I/O fields populated.

### Implementation for User Story 3

- [X] T023 [P] [US3] Create `src/infra/win/idle.rs` — `WinIdleProbe` struct with `probe() -> IdleProbeResult` using `GetLastInputInfo` + `GetForegroundWindow` via `windows-sys`; compile-gated `#[cfg(windows)]`
- [X] T024 [P] [US3] Create `src/ui/diagnose.rs` — `print_diagnose(report: &DiagnoseReport)` function that prints sorted table `Category | Size | Risk | Reclaim` + summary line `potential reclaim: <total> (Safe <safe_total>)`; empty state prints `no cleanable categories found` + `potential reclaim: 0 B`
- [X] T025 [US3] Create `src/services/diagnose_service.rs` — `DiagnoseService` with `build_report(categories: &[CleanCategory], scans: &[CategoryScan]) -> DiagnoseReport` that maps each scan to `DiagnoseRow` with `RiskLevel::Safe`, sorts by `size_bytes` desc, sums `total_reclaimable`
- [X] T026 [US3] Wire `run_diagnose` in `src/main.rs` — call `discover_categories()` + `CleanService::scan()` + `DiagnoseService::build_report()` + `print_diagnose()`
- [X] T027 [US3] Add unit test for `DiagnoseService::build_report` — verify sorting, rollup sum, empty input handling in `src/services/diagnose_service.rs`
- [X] T028 [US3] Add unit test for `print_diagnose` output format — verify column alignment, summary line format, empty state in `src/ui/diagnose.rs`

**Checkpoint**: `sweep diagnose` prints full sorted table with rollup. Per-process I/O in snapshot. Idle probe skeleton on Windows (stub on Linux).

---

## Phase 6: User Story 4 — Before/After Benchmark (Priority: P3)

**Goal**: Every `sweep clean` and `sweep bin --empty` prints `before X free → after Y free (freed Z) in Ns`.

**Independent Test**: Run clean and bin, verify benchmark line appears with correct format.

### Implementation for User Story 4

- [X] T029 [US4] Create `print_benchmark(sample: &BenchmarkSample)` function in `src/ui/clean.rs` — prints `before {fmt(before_free)} free → after {fmt(after_free)} free (freed {fmt(freed)}) in {elapsed}s`
- [X] T030 [US4] Wrap `run_clean` trash operation in `src/main.rs` with `Instant::now()` + `free_bytes_on_index_volume()` before/after snapshot, call `print_benchmark()`
- [X] T031 [US4] Wrap `run_bin` purge operation in `src/main.rs` with `Instant::now()` + `free_bytes_on_index_volume()` before/after snapshot, call `print_benchmark()`
- [X] T032 [US4] Add unit test for `BenchmarkSample::freed_bytes()` and `print_benchmark` format in `src/ui/clean.rs`

**Checkpoint**: Every clean/bin shows before/after benchmark. `freed = after - before`. `elapsed >= 0`.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: CI gate, documentation, final verification.

- [X] T033 Run `cargo test --locked` and fix any compilation or test failures
- [X] T034 Run `cargo build --release` and verify binary builds on both OS
- [X] T035 [P] Update `README.md` to document `sweep diagnose` command and reserve behavior
- [X] T036 [P] Update `ROADMAP.md` to mark Stage 1 features as complete
- [X] T037 Run quickstart.md validation scenarios V1–V10 manually or via integration tests

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on T001–T004 (domain models + paths + Cargo.toml)
- **Phase 3 (US1)**: Depends on T003 (reserve helpers), T007 (empty-only fix), T008 (CLI diagnose wired)
- **Phase 4 (US2)**: Depends on T006 (mod.rs declarations) — can run parallel with Phase 3
- **Phase 5 (US3)**: Depends on T005 (sysinfo disk_usage), T008 (CLI diagnose), T018 (dev_caches module)
- **Phase 6 (US4)**: Depends on T003 (free_bytes_on_index_volume), T008 (CLI)
- **Phase 7 (Polish)**: Depends on all previous phases

### User Story Dependencies

- **US1 (P1)**: Can start after Phase 2 — no dependency on other stories
- **US2 (P1)**: Can start after Phase 2 — no dependency on other stories
- **US3 (P2)**: Depends on US2 (dev_caches module must exist for diagnose to scan it)
- **US4 (P3)**: Can start after Phase 2 — independent of US2/US3

### Within Each User Story

- Models/helpers before services
- Services before UI/main.rs wiring
- Tests alongside or immediately after implementation
- Story complete before moving to next priority

### Parallel Opportunities

- **Phase 1**: T001, T002, T004 can all run in parallel (different files)
- **Phase 3 + Phase 4**: US1 and US2 can be implemented in parallel (different files: US1 touches `main.rs`/`paths.rs`, US2 touches `dev_caches.rs`/`clean_paths.rs`)
- **Phase 5**: T023 and T024 can run in parallel (different files: `idle.rs` vs `diagnose.rs`)
- **Phase 7**: T035 and T036 can run in parallel (different files)

---

## Parallel Example: User Story 1 + User Story 2

```bash
# US1 and US2 can be developed simultaneously:

# US1 tasks (main.rs, paths.rs):
Task T009: "Implement open_store_with_reserve() in src/main.rs"
Task T010: "Modify run_status fallback in src/main.rs"
Task T015: "Add SWEEP_DB env in src/infra/paths.rs"

# US2 tasks (dev_caches.rs, clean_paths.rs):
Task T018: "Create src/infra/dev_caches.rs"
Task T019: "Add dev categories to src/infra/win/clean_paths.rs"
Task T020: "Add dev categories to src/infra/linux/clean_paths.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T004)
2. Complete Phase 2: Foundational (T005–T008)
3. Complete Phase 3: User Story 1 (T009–T017)
4. **STOP and VALIDATE**: `sweep status` works at 0 B, reserve lifecycle verified
5. `cargo test --locked` green

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. US1 (P1) → Status survives 0 B, reserve lifecycle complete — **MVP!**
3. US2 (P1) → Dev caches discovered on both OS — **Reclaim bloat!**
4. US3 (P2) → Diagnose command with sorted table — **Visibility!**
5. US4 (P3) → Before/after benchmarks — **Trust!**

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Total tasks: 37 (4 setup + 4 foundational + 9 US1 + 5 US2 + 6 US3 + 4 US4 + 5 polish)
