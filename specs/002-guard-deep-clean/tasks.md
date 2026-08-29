# Tasks: Guard Daemon & Deep System Cleaning (Stage 2)

**Input**: Design documents from `/specs/002-guard-deep-clean/`

**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Domain model extensions and path helpers that ALL user stories depend on

- [x] T001 [P] Add `risk: RiskLevel` field to `CleanCategory` in `src/domain/models.rs` and update all `discover_categories()` call sites in `src/infra/win/clean_paths.rs` and `src/infra/linux/clean_paths.rs` to pass `risk: RiskLevel::Safe`
- [x] T002 [P] Add new domain models in `src/domain/models.rs`: `GuardConfig`, `RamSnapshot`, `DiskSnapshot`, `ServiceGuardState`, `ServiceEntry`, `IdleSsdOffender`, `IdleReason`, `GuardBenchmark`, `GuardAction`, `DiskRescuePhase`, `GuardLogEntry`, `DeepScanResult`
- [x] T003 [P] Extend `BenchmarkSample` in `src/domain/models.rs` with `category_bytes: Vec<(String, u64)>` field and add `safe_freed()` / `system_freed()` methods that filter by category risk level
- [x] T004 [P] Extend `DiagnoseReport` in `src/domain/models.rs` with `safe_reclaimable: u64` and `system_reclaimable: u64` fields
- [x] T005 [P] Add `GuardMonitor` trait in `src/domain/traits.rs` with `fn snapshot_ram(&self) -> Result<RamSnapshot>` and `fn snapshot_disk(&self) -> Result<DiskSnapshot>`
- [x] T006 [P] Implement `GuardMonitor` for `SysinfoMonitor` in `src/infra/sysinfo_monitor.rs` (snapshot RAM via sysinfo, snapshot disk via `free_bytes_on_index_volume`)
- [x] T007 [P] Add `guard_log_path()` and `guard_lock_path()` helpers in `src/infra/paths.rs`
- [x] T008 [P] Add `Guard` CLI command variant in `src/ui/cli.rs` with `--ram-threshold`, `--disk-min-gb`, `--interval-secs`, `--once`, `--allow-service-stop`, `--allow-kill` flags
- [x] T009 [P] Add `Idle` CLI command variant in `src/ui/cli.rs` with `--top`, `--idle-mins`, `--min-write-mb`, `--clean-cache` flags
- [x] T010 [P] Add `--deep` flag to `Diagnose` and `Clean` command variants in `src/ui/cli.rs`
- [x] T011 [P] Add `--stop-services` flag to `Clean` command variant in `src/ui/cli.rs`
- [x] T012 [P] Add wire-up stubs in `src/main.rs` for `run_guard`, `run_idle`, extended `run_clean`/`run_diagnose` with `--deep`/`--stop-services`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Cross-cutting infrastructure that user stories 1 and 2+ depend on

- [x] T013 Implement guard log writer in `src/services/guard_service.rs` — `GuardLog::write()` appends timestamped entries to `%LOCALAPPDATA%\sweep\guard.log` with format `[ISO8601] [LEVEL] [message] [bytes_freed]`
- [x] T014 Implement Windows toast notification in `src/ui/guard.rs` — `send_toast(title, body)` invokes PowerShell WinRT one-liner, returns `Result<()>` with graceful no-op on failure
- [x] T015 Implement file-based single-instance mutex in `src/services/guard_service.rs` — `GuardLock` struct that acquires exclusive file lock on `guard_lock_path()`, returns error if already locked

---

## Phase 3: User Story 1 — Guard Prevents System Hang (Priority: P1) 🎯 MVP

**Goal**: Background daemon monitors RAM/disk, trims working sets on pressure, performs graduated disk rescue, enforces cooldown, logs everything, shows toasts

**Independent Test**: `sweep guard --once` with RAM ≥90% triggers trim; `sweep guard --once --disk-min-gb 999` triggers disk rescue; guard.log has entries; toast fires

### Implementation for User Story 1

- [x] T016 [US1] Implement `GuardService::run()` in `src/services/guard_service.rs` — main poll loop with `interval_secs` sleep, RAM/disk snapshot each cycle, 3-sample hysteresis counter, cooldown timer
- [x] T017 [US1] Implement RAM pressure detection and trim in `src/services/guard_service.rs` — when 3 consecutive samples ≥ threshold, call `RamService::optimize(Some(10), true)`, log result, enter cooldown
- [x] T018 [US1] Implement disk rescue graduated response in `src/services/guard_service.rs` — Phase 1: `consume_reserve()`, Phase 2: `CleanService::run()` on safe-only categories, Phase 3: `TrashBin::purge_all()`; check free space after each phase, exit early if above threshold
- [x] T019 [US1] Implement `--once` mode in `src/services/guard_service.rs` — run one poll cycle and exit (no loop)
- [x] T020 [US1] Wire up guard toast notifications in `src/services/guard_service.rs` — call `send_toast()` after each RAM trim and disk rescue action
- [x] T021 [US1] Wire up guard log entries in `src/services/guard_service.rs` — write `GuardLogEntry` for every action, toast result, cooldown start
- [x] T022 [US1] Wire up `run_guard` in `src/main.rs` — parse `GuardConfig` from CLI args, acquire `GuardLock`, call `GuardService::run()`, handle lock failure with exit code 1
- [x] T023 [US1] Implement guard stdout output in `src/ui/guard.rs` — `print_guard_cycle()` for `--once` mode showing poll status, actions taken, freed bytes

**Checkpoint**: `sweep guard --once` fully functional — RAM trim, disk rescue, cooldown, toast, logging, single-instance lock all working

---

## Phase 4: User Story 2 — Deep System Scan (Priority: P2)

**Goal**: Discover WU downloads, DO cache, WinSxS reclaimable, driver store; show with `sweep diagnose --deep`; scan-only by default

**Independent Test**: `sweep diagnose --deep` shows WU/DO/WinSxS/drivers with risk=System; `sweep clean --scan-only --deep` reports same without deleting

### Implementation for User Story 2

- [x] T024 [P] [US2] Implement `DeepCleanService::scan()` in `src/infra/win/deep_clean.rs` — discover WU download size (`SoftwareDistribution\Download`), DO cache size, driver store size + oldest age; all `#[cfg(windows)]`
- [x] T025 [US2] Implement WinSxS analysis in `src/infra/win/deep_clean.rs` — run `dism /Online /Cleanup-Image /AnalyzeComponentStore`, parse output for reclaimable bytes, return `None` if dism unavailable or access denied
- [x] T026 [US2] Add deep system categories to `discover_categories()` in `src/infra/win/clean_paths.rs` — append WU downloads, DO cache, driver store with `risk: RiskLevel::System`; gated on `deep: bool` parameter
- [x] T027 [US2] Extend `run_diagnose` in `src/ui/diagnose.rs` and `src/main.rs` — when `--deep` flag set, include System categories, compute `safe_reclaimable` + `system_reclaimable`, print split in summary line
- [x] T028 [US2] Extend `run_clean` in `src/main.rs` — when `--deep` flag set, include System categories in scan; when `--scan-only`, show all categories without deleting; when cleaning without `--stop-services`, skip locked WU category
- [x] T029 [US2] Add `#[cfg(not(windows))]` stubs for deep_clean — `DeepCleanService::scan()` returns zeroed `DeepScanResult` on Linux

**Checkpoint**: `sweep diagnose --deep` shows WU/DO/WinSxS/drivers; `sweep clean --scan-only --deep` reports sizes without deleting

---

## Phase 5: User Story 3 — Service-Aware Unlock (Priority: P3)

**Goal**: RAII service stop/start for wuauserv+bits, opt-in via `--stop-services`, guard never auto-stops

**Independent Test**: `sweep clean --deep --stop-services -y` stops services, cleans WU downloads, restores services; guard never auto-stops

### Implementation for User Story 3

- [x] T030 [US3] Implement `ServiceGuard` struct in `src/infra/win/service_lock.rs` — `new(services: &[&str])` opens SCM, stops each service, records `was_running`; `Drop` impl starts services that were running; 30s timeout on stop; `#[cfg(windows)]`
- [x] T031 [US3] Integrate `ServiceGuard` into `run_clean` in `src/main.rs` — when `--deep` AND `--stop-services` both set, create `ServiceGuard` before cleaning, let Drop restore after
- [x] T032 [US3] Ensure guard never auto-stops services — verify `GuardService` does not reference `ServiceGuard`; add `allow_service_stop` check in guard disk rescue path
- [x] T033 [US3] Add `#[cfg(not(windows))]` stub for `ServiceGuard` — no-op struct that does nothing on Drop

**Checkpoint**: `sweep clean --deep --stop-services -y` safely stops/restores services; guard never auto-stops

---

## Phase 6: User Story 4 — Idle SSD Offender Detection (Priority: P4)

**Goal**: Two-snapshot I/O diff, identify idle heavy writers, display table, optional cache clean

**Independent Test**: `sweep idle` shows table with PID/APP/IDLE/WRITE/h/RAM/REASON; `--clean-cache` cleans whitelisted cache

### Implementation for User Story 4

- [x] T034 [US4] Implement `IdleService::detect()` in `src/services/idle_service.rs` — take two sysinfo snapshots 60s apart, compute write_delta per process, filter by idle_mins + min_write_mb thresholds, exclude foreground PID, classify reason
- [x] T035 [US4] Implement foreground PID detection in `src/services/idle_service.rs` — `#[cfg(windows)]` use `GetForegroundWindow` + `GetWindowThreadProcessId` (reuse existing `infra/win/idle.rs`), `#[cfg(not(windows))]` use current process heuristic
- [x] T036 [US4] Implement idle cache cleaning in `src/services/idle_service.rs` — `clean_cache(offenders)` maps offender names to known cache dirs (reuse `infra/dev_caches.rs` patterns), trashes items via `TrashRemover`
- [x] T037 [US4] Implement idle table output in `src/ui/idle.rs` — `print_idle_table(offenders, total_writes)` with columns PID | APP | IDLE | WRITE/h | RAM | REASON; empty state message
- [x] T038 [US4] Wire up `run_idle` in `src/main.rs` — parse config from CLI, call `IdleService::detect()`, print table, optionally clean cache

**Checkpoint**: `sweep idle` detects and displays idle heavy writers; `--clean-cache` cleans their caches

---

## Phase 7: User Story 5 — Full Benchmark Visibility (Priority: P5)

**Goal**: Every clean/guard logs before/after + per-category; diagnose shows Safe/System split

**Independent Test**: `sweep clean` output has benchmark with per-category breakdown; `sweep diagnose --deep` shows Safe X + System Y

### Implementation for User Story 5

- [x] T039 [US5] Implement `BenchmarkRecorder` in `src/services/benchmark.rs` — `start()` captures `before_free_bytes`, `record_category(id, bytes)` tracks per-category, `finish()` returns `BenchmarkSample` with elapsed + category_bytes
- [x] T040 [US5] Integrate benchmark recording into `run_clean` in `src/main.rs` — wrap clean operation with `BenchmarkRecorder`, print benchmark result with per-category breakdown after clean
- [x] T041 [US5] Integrate benchmark recording into guard disk rescue in `src/services/guard_service.rs` — record before/after + per-category bytes for each rescue phase, write to guard.log
- [x] T042 [US5] Update `print_benchmark()` in `src/ui/clean.rs` — print per-category breakdown below the before/after line, format: `  category_id: X GiB`
- [x] T043 [US5] Update `print_diagnose` in `src/ui/diagnose.rs` — print Safe/System split in summary: `potential reclaim: X (Safe Y, System Z)`

**Checkpoint**: All clean/guard operations show benchmark with per-category breakdown; diagnose shows Safe/System split

---

## Phase 8: User Story 6 — Guard Autostart via Schedule (Priority: P6)

**Goal**: `sweep schedule --guard-install` registers ONLOGON task for `sweep guard`

**Independent Test**: `sweep schedule --guard-install` creates task; `sweep schedule --guard-status` shows installed; `sweep schedule --guard-remove` removes it

### Implementation for User Story 6

- [x] T044 [US6] Add `GUARD_TASK_NAME` const and `guard_create_args`/`guard_delete_args`/`guard_query_args` functions in `src/infra/schedule.rs` — schtasks `/SC ONLOGON /TN SweepGuard /TR "sweep guard"`
- [x] T045 [US6] Add `guard_install()`, `guard_remove()`, `guard_is_installed()` public functions in `src/infra/schedule.rs` — reuse existing `windows_impl::run()` helper
- [x] T046 [US6] Add `--guard-install`/`--guard-remove`/`--guard-status` flags to `Schedule` command in `src/ui/cli.rs` and wire up in `src/main.rs`
- [x] T047 [US6] Add Linux stub for guard scheduling in `src/infra/schedule.rs` — `guard_install()` returns error "autostart not supported on Linux"

**Checkpoint**: Guard autostart registers and removes logon scheduled task on Windows

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final integration, validation, and cleanup

- [x] T048 Run `cargo test --locked` and fix any compilation errors from all new models, traits, and modules
- [x] T049 Run `cargo build --release` and verify binary builds successfully
- [ ] T050 Run quickstart.md validation scenarios V1–V13 end-to-end on Windows
- [ ] T051 Verify guard RSS < 50 MB and idle CPU < 1% during polling (manual measurement or unit test)
- [x] T052 Add doc comments (Principle VII) to all new public items: `GuardConfig`, `GuardService`, `ServiceGuard`, `IdleService`, `DeepCleanService`, `BenchmarkRecorder`, trait impls, new CLI variants
- [x] T053 Update `README.md` with Stage 2 features: guard daemon, deep scan, idle detection, benchmark, autostart

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — can start immediately. All tasks are independent [P].
- **Phase 2 (Foundational)**: Depends on Phase 1 completion (needs domain models and paths). T013–T015 can run in parallel.
- **Phase 3 (US1 — Guard)**: Depends on Phases 1–2. Core MVP.
- **Phase 4 (US2 — Deep Scan)**: Depends on Phases 1–2. Can run in parallel with Phase 3 (different files).
- **Phase 5 (US3 — Service Lock)**: Depends on Phase 4 (needs deep categories wired up).
- **Phase 6 (US4 — Idle)**: Depends on Phases 1–2 only. Can run in parallel with Phases 3–5.
- **Phase 7 (US5 — Benchmark)**: Depends on Phases 1–2. Can run in parallel with Phases 3–6, but integrates into clean/guard flows.
- **Phase 8 (US6 — Autostart)**: Depends on Phase 3 (needs guard working). Can run in parallel with Phases 4–7.
- **Phase 9 (Polish)**: Depends on all desired user stories being complete.

### User Story Dependencies

- **US1 (Guard, P1)**: Can start after Phase 2 — no dependencies on other stories
- **US2 (Deep Scan, P2)**: Can start after Phase 2 — independent of US1
- **US3 (Service Lock, P3)**: Depends on US2 (needs deep categories in discover_categories)
- **US4 (Idle, P4)**: Can start after Phase 2 — fully independent
- **US5 (Benchmark, P5)**: Can start after Phase 2 — integrates into US1 and US2 flows but structurally independent
- **US6 (Autostart, P6)**: Depends on US1 (needs guard working to register)

### Within Each User Story

- Models/entities first (Phase 1 handles this)
- Service layer before UI wiring
- Core implementation before integration with other stories
- Story complete before moving to next priority

### Parallel Opportunities

- All Phase 1 tasks (T001–T012) can run in parallel — different files, no deps
- Phase 2 tasks (T013–T015) can run in parallel — different files
- US1 (Phase 3) and US2 (Phase 4) can run in parallel after Phase 2
- US4 (Phase 6) can run in parallel with US1, US2, US3, US5
- US5 (Phase 7) can run in parallel with US1–US4, US6
- US6 (Phase 8) can run in parallel with US2–US5

---

## Parallel Example: Phase 1 (Setup)

```bash
# All 12 setup tasks can launch together:
Task T001: "Add risk field to CleanCategory in src/domain/models.rs + update discover_categories() call sites"
Task T002: "Add new domain models in src/domain/models.rs"
Task T003: "Extend BenchmarkSample in src/domain/models.rs"
Task T004: "Extend DiagnoseReport in src/domain/models.rs"
Task T005: "Add GuardMonitor trait in src/domain/traits.rs"
Task T006: "Implement GuardMonitor for SysinfoMonitor in src/infra/sysinfo_monitor.rs"
Task T007: "Add guard_log_path/guard_lock_path in src/infra/paths.rs"
Task T008: "Add Guard CLI command in src/ui/cli.rs"
Task T009: "Add Idle CLI command in src/ui/cli.rs"
Task T010: "Add --deep flag to Diagnose/Clean in src/ui/cli.rs"
Task T011: "Add --stop-services flag to Clean in src/ui/cli.rs"
Task T012: "Add wire-up stubs in src/main.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T012)
2. Complete Phase 2: Foundational (T013–T015)
3. Complete Phase 3: US1 — Guard (T016–T023)
4. **STOP and VALIDATE**: Test `sweep guard --once` end-to-end
5. Ship MVP — guard daemon prevents hangs

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. US1 (Guard) → Test independently → **MVP!**
3. US2 (Deep Scan) → Test with `diagnose --deep` → **Deep visibility!**
4. US3 (Service Lock) → Test with `clean --deep --stop-services` → **Full deep clean!**
5. US4 (Idle) → Test with `sweep idle` → **SSD protection!**
6. US5 (Benchmark) → Verify all operations show before/after → **Trust!**
7. US6 (Autostart) → Test `schedule --guard-install` → **Set-and-forget!**

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Phase 1 touches many files but each task is isolated to specific structs/traits
- Constitution VII (doc comments) is addressed in Phase 9 (T052) — apply to all new public items
- Windows-only code (`deep_clean.rs`, `service_lock.rs`) gets `#[cfg(not(windows))]` stubs in same task
