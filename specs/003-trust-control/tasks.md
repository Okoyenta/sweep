# Tasks: Trust & Control (Stage 3)

**Input**: Design documents from `/specs/003-trust-control/`

**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/cli.md, quickstart.md

**Tests**: Tests are OPTIONAL per the template. This feature's spec DOES require CI verification (SC-010) and Constitution Principle IV (Test-First & Verification), so a small number of unit-test tasks are included per story where they are cheap and OS-independent. Live/OS-state probes must be `#[ignore]`d.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**State note**: Phases 1 and 2 already exist in the working tree (domain models, `infra/exclusions.rs`, `infra/rulepack.rs`, `infra/undo.rs`, `infra/{win,linux}/doctor.rs`, `process_lock` close/kill, `exclusion_service.rs`, `kill_service.rs`, `undo_service.rs`) and are marked `[X]`. None of it is wired into `ui/` or `main.rs` yet — that is what Phases 3+ deliver.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: single Rust crate `sweep`, sources under `src/` at repository root
- Layering per Constitution Principle III: `domain/` → `services/` → `infra/` → `ui/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies and domain types every story depends on

- [X] T001 Add `toml = "0.8"` dependency in Cargo.toml for `sweep.toml` + rule-pack parsing (research R4)
- [X] T002 [P] Add `CategoryEstimate` struct in src/domain/models.rs (id, size_bytes, risk)
- [X] T003 [P] Add `DoctorReport` struct plus `ReserveStatus` / `ElevationStatus` / `ToastStatus` enums in src/domain/models.rs
- [X] T004 [P] Add `ExclusionConfig` struct in src/domain/models.rs (paths, category_ids, globs)
- [X] T005 [P] Add `RulePackCategory` struct in src/domain/models.rs (id, roots, risk, cleanup_command)
- [X] T006 [P] Add `UndoJournal` / `UndoSession` / `UndoItem` structs in src/domain/models.rs
- [X] T007 [P] Add `KillRequest` struct and `KillMode` enum in src/domain/models.rs (pid, name, size_bytes, mode, consent)
- [X] T008 [P] Add `TuiView` enum in src/domain/models.rs (Background, Idle, KillModal)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Cross-platform infra and services that every Stage 3 story builds on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T009 Create src/infra/exclusions.rs — `resolve_toml_path()` (`--config` > `./sweep.toml` > user config dir) and `load_exclusions()` returning an empty config on missing/invalid TOML (research R1, FR-004, FR-017)
- [X] T010 [P] Create src/infra/rulepack.rs — `load_rule_packs()` parsing `[[category]]` entries into `RulePackCategory`, skipping invalid entries with a warning (FR-015, FR-017)
- [X] T011 [P] Create src/infra/undo.rs — append-only JSON journal (`journal_path`, `read_journal`, `write_journal`, `append_session`, `restore_latest`, `UndoResult`) (research R2, FR-006..008)
- [X] T012 [P] Extend src/infra/paths.rs — expose the sweep config/data directories used by exclusions.rs and undo.rs
- [X] T013 Create src/services/exclusion_service.rs — `apply_exclusions()` (path/category/glob filtering + excluded count) and `load_policy()` as the single entry point for diagnose/clean/guard (FR-005)
- [X] T014 [P] Create src/services/kill_service.rs — `KillService` with the hard blocklist (PID 0/4, csrss, wininit, services, self PID) and consent gating for `KillMode::Kill` (research R7, FR-011)
- [X] T015 [P] Extend src/infra/win/process_lock.rs — add `graceful_close(pid)` (WM_CLOSE) and `kill(pid)` (taskkill /F) (research R8)
- [X] T016 [P] Extend src/infra/linux/process_lock.rs — add `graceful_close(pid)` (SIGTERM) and `kill(pid)` (SIGKILL) (research R8, Principle V)
- [X] T017 [P] Create src/infra/win/doctor.rs — elevation probe and toast-availability probe (research R5, R6)
- [X] T018 [P] Create src/infra/linux/doctor.rs — `geteuid() == 0` elevation probe; toast always `Unavailable` (research R5, Principle V)
- [X] T019 Create src/services/undo_service.rs — `UndoService` wrapping `infra::undo::restore_latest()` into an `UndoOutcome` for the UI layer (FR-007, FR-008)
- [X] T020 Register the new modules in src/infra/mod.rs, src/infra/win/mod.rs, src/infra/linux/mod.rs, and src/services/mod.rs

**Checkpoint**: Foundation ready — user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Doctor Pre-Flight Builds Trust (Priority: P1) 🎯 MVP

**Goal**: `sweep doctor` prints a single pre-flight safety report — reserve status, elevation, toast, guard armed state, idle offender count, and a would-clean size/category list.

**Independent Test**: Run `cargo run -- doctor` on a healthy system; it prints `reserve:`, `elevation:`, `toast:`, `guard:`, `idle: <N> offenders`, and `would-clean: <size> across <M> categories` with per-category lines, exiting 0 within 5 seconds (quickstart Q1, SC-001).

### Implementation for User Story 1

- [X] T021 [US1] Create src/services/doctor_service.rs — `DoctorService::report()` assembling a `DoctorReport` from reserve state, elevation probe, toast probe, guard-armed read, would-clean estimate, and idle offender count (FR-001..003)
- [X] T022 [US1] In src/services/doctor_service.rs, implement reserve-status detection (`Ok` / `Missing` / `Consumed`) by reading the reserve file via src/infra/paths.rs (FR-001)
- [X] T023 [US1] In src/services/doctor_service.rs, implement guard-armed detection by querying the logon task/unit via src/infra/schedule.rs (FR-002)
- [X] T024 [US1] In src/services/doctor_service.rs, compute `would_clean` + `would_clean_total_bytes` by reusing the clean-category discovery used by src/services/clean_service.rs (FR-002)
- [X] T025 [US1] In src/services/doctor_service.rs, compute `idle_offender_count` by reusing src/services/idle_service.rs (FR-003)
- [X] T026 [US1] Register `pub mod doctor_service;` in src/services/mod.rs
- [X] T027 [P] [US1] Create src/ui/doctor.rs rendering `DoctorReport` in the exact stable field order from specs/003-trust-control/contracts/cli.md, and register it in src/ui/mod.rs
- [X] T028 [US1] Add the `Doctor` subcommand (no flags) to the `Commands` enum in src/ui/cli.rs (FR-001..003)
- [X] T029 [US1] Route `Commands::Doctor` in src/main.rs to `DoctorService::report()` + the src/ui/doctor.rs printer, always exiting 0 (contracts/cli.md)
- [X] T030 [P] [US1] Add a unit test in src/services/doctor_service.rs asserting `would_clean_total_bytes == sum(would_clean[].size_bytes)` (data-model.md validation)
- [X] T031 [P] [US1] Add doc comments to every new public item in src/services/doctor_service.rs and src/ui/doctor.rs (FR-021, Principle VII)

**Checkpoint**: User Story 1 fully functional and testable independently — MVP complete

---

## Phase 4: User Story 2 - Per-User Exclusions via sweep.toml (Priority: P2)

**Goal**: `[exclusions]` in `sweep.toml` (paths, category_ids, globs) is honored by diagnose, clean, and guard, pruned before size calculation and logged.

**Independent Test**: Add `category_ids = ["dev-pnpm"]` to `./sweep.toml`, run `cargo run -- diagnose` and `cargo run -- clean --scan-only`; `dev-pnpm` is absent and an `excluded: N` line is printed. Delete `sweep.toml` → it reappears (quickstart Q2, SC-002).

### Implementation for User Story 2

- [X] T032 [US2] Call `exclusion_service::load_policy()` in src/services/clean_service.rs and filter discovered categories before sizing (FR-005)
- [X] T033 [US2] Call `exclusion_service::load_policy()` in src/services/diagnose_service.rs and drop excluded categories/paths from the report (FR-004, FR-005)
- [X] T034 [US2] Call `exclusion_service::load_policy()` in the disk-rescue path of src/services/guard_service.rs so unattended cleaning honors exclusions (FR-005, contracts/cli.md)
- [X] T035 [US2] Prune excluded candidate items before size calculation so excluded trees are never walked (FR-005, US2 acceptance #4). **Landed in `CleanService::scan_excluding` (src/services/clean_service.rs), not src/infra/walker.rs**: `walker.rs` serves the index, while clean/doctor/guard size categories through `clean_service`, so that is where pruning actually precedes measurement.
- [X] T036 [P] [US2] Print an `excluded: N` line in src/ui/clean.rs and src/ui/diagnose.rs when exclusions were applied (FR-005, Principle VI)
- [X] T037 [US2] Log excluded categories to the guard audit log in src/services/guard_service.rs (Principle VI)
- [X] T038 [US2] Add a global `--config <path>` flag to src/ui/cli.rs and thread it through to `load_policy()` in src/main.rs (research R1)
- [X] T039 [P] [US2] Add unit tests in src/services/exclusion_service.rs covering category-id exclusion, path-prefix exclusion, `**` glob matching, and empty-config passthrough (FR-004, US2 acceptance #3)
- [X] T040 [P] [US2] Add a unit test in src/infra/exclusions.rs asserting malformed TOML yields an empty `ExclusionConfig` without panicking (FR-017, edge case)

**Checkpoint**: User Stories 1 and 2 both work independently

---

## Phase 5: User Story 3 - Undo Journal for Trashed Items (Priority: P3)

**Goal**: `sweep clean` journals every trashed item; `sweep undo` restores the newest session or reports items as unrecoverable when the Recycle Bin was purged.

**Independent Test**: `cargo run -- clean -y --only user-temp` then `cargo run -- undo` restores the items to their original paths; purge the Recycle Bin and re-run → `unrecoverable (recycle bin purged)`, exit 0 (quickstart Q3, SC-003).

### Implementation for User Story 3

- [X] T041 [US3] Record each successful trash move as an `UndoItem` (original_path, trash_path) in src/infra/trash_remover.rs (FR-006)
- [X] T042 [US3] Call `infra::undo::append_session()` once per clean run in src/services/clean_service.rs with the collected items (FR-006)
- [X] T043 [P] [US3] Create src/ui/undo.rs rendering `UndoOutcome` as per-item `restored` / `unrecoverable (recycle bin purged)` plus a restored count, and register it in src/ui/mod.rs (FR-007, FR-008)
- [X] T044 [US3] Add the `Undo` subcommand (no args) to the `Commands` enum in src/ui/cli.rs (contracts/cli.md)
- [X] T045 [US3] Route `Commands::Undo` in src/main.rs to `UndoService`, printing "no session to undo" and exiting 0 when the journal is empty (FR-007, edge case)
- [X] T046 [P] [US3] Add unit tests in src/infra/undo.rs covering append/read round-trip, newest-session selection, and corrupt-JSON → fresh journal (data-model.md validation, US3 acceptance #3)
- [X] T047 [P] [US3] Add doc comments to the new public items in src/ui/undo.rs and the src/infra/trash_remover.rs changes (FR-021)

**Checkpoint**: User Stories 1-3 all work independently

---

## Phase 6: User Story 4 - Controlled Process Termination (Priority: P4)

**Goal**: Graduated, consent-gated termination — `sweep idle --close` graceful, `sweep idle --kill --force` / `sweep bg --kill --force` behind a per-process confirm, blocklist always wins, guard `--allow-kill` graceful-only.

**Independent Test**: Launch an idle app; `cargo run -- idle --close --only <pid>` exits it cleanly; `cargo run -- idle --kill --force --only <pid>` prompts `kill <name> PID <pid> <size>?` and only kills on approval; PID 4 / `csrss` are always skipped (quickstart Q4, SC-004, SC-005).

### Implementation for User Story 4

- [X] T048 [US4] Add `--close`, `--kill`, `--force`, and `--only <pid>...` flags to the `Idle` subcommand in src/ui/cli.rs (FR-009, FR-010, contracts/cli.md)
- [X] T049 [US4] Add the `Bg` subcommand (`--top N`, `--kill`, `--force`, `--only <pid>...`) to the `Commands` enum in src/ui/cli.rs (contracts/cli.md)
- [X] T050 [US4] Implement `confirm(prompt) -> bool` in src/ui/idle.rs (or reuse the existing prompt in src/ui/guard.rs) rendering `kill <name> PID <pid> <size>?` (FR-010)
- [X] T051 [US4] Route `sweep idle --close` in src/main.rs to `KillService` with `KillMode::Close` for the selected offenders (FR-009)
- [X] T052 [US4] Route `sweep idle --kill --force` in src/main.rs — build a `KillRequest` per target, require `confirm()` to set `consent`, then execute (FR-010)
- [X] T053 [US4] Route `sweep bg` in src/main.rs to src/services/ram_service.rs for the background list and to `KillService` for `--kill --force`, using the same consent + blocklist rules (contracts/cli.md)
- [X] T054 [US4] Report already-exited PIDs as "already gone" and skip them without error in src/services/kill_service.rs (edge case)
- [X] T055 [US4] Implement the `--allow-kill` guard path in src/services/guard_service.rs — graceful close (tier 2) only for idle offenders writing `>500 MB/h` for `>60m`, never a forced kill (FR-012)
- [X] T056 [US4] Ensure guard without `--allow-kill` never closes or kills any process (trim-only) in src/services/guard_service.rs (FR-013, Principle II)
- [X] T057 [US4] Log every close/kill decision — target, mode, consent, blocklist skip — to the guard audit log in src/services/guard_service.rs (Principle II, Principle VI)
- [X] T058 [P] [US4] Add unit tests in src/services/kill_service.rs asserting `is_blocked` for PID 0, PID 4, `csrss`, `wininit`, `services`, and self PID, and that `KillMode::Kill` without consent is rejected (FR-011, US4 acceptance #3)
- [X] T059 [P] [US4] Add doc comments to the new routing in src/main.rs, `confirm()` in src/ui/idle.rs, and the guard `--allow-kill` path (FR-021)

**Checkpoint**: User Stories 1-4 all independently functional

---

## Phase 7: User Story 5 - TUI Background / Idle / Kill Views (Priority: P5)

**Goal**: TUI `b` (background), `i` (idle offenders), and `k` (kill confirmation modal) respecting the blocklist.

**Independent Test**: `cargo run -- tui`, press `b` → background list, `i` → idle table (PID, APP, IDLE, WRITE/h, RAM, REASON), select + `k` → confirmation modal; a blocklisted PID is refused (quickstart Q5, SC-006).

### Implementation for User Story 5

- [X] T060 [US5] Add `TuiView` + `selected_pid` state to the app struct in src/ui/tui.rs (data-model.md)
- [X] T061 [US5] Add the `b` key handler and background-process (RAM/disk writer) table rendering in src/ui/tui.rs (FR-014)
- [X] T062 [US5] Add the `i` key handler and idle-offender table (PID, APP, IDLE, WRITE/h, RAM, REASON) in src/ui/tui.rs (FR-014)
- [X] T063 [US5] Add up/down list selection for the background and idle views in src/ui/tui.rs
- [X] T064 [US5] Add the `k` key handler and confirmation modal in src/ui/tui.rs, dispatching a `KillRequest` through `KillService` only on confirm (FR-014)
- [X] T065 [US5] Show a "protected system process" refusal instead of the modal for blocklisted selections in src/ui/tui.rs (FR-011, FR-014)
- [X] T066 [US5] Extend the `TuiAction` enum and its handler in src/main.rs to carry the close/kill actions (Principle III — UI never calls infra directly)
- [X] T067 [P] [US5] Update the TUI key legend/help line in src/ui/tui.rs to include `b`, `i`, `k`
- [X] T068 [P] [US5] Add doc comments to the new TUI view/state items in src/ui/tui.rs (FR-021)

**Checkpoint**: User Stories 1-5 all independently functional

---

## Phase 8: User Story 6 - Cleaner Rule Packs (TOML, No Code) (Priority: P6)

**Goal**: User-supplied `[[category]]` entries are merged into discovery with built-in risk/visibility policy and safe fallback on invalid input.

**Independent Test**: Add a `myapp-cache` `[[category]]` with a pre-populated root to `sweep.toml`, run `cargo run -- clean --scan-only` → it appears with a computed size and `Safe` risk; switch to `risk = "System"` → hidden without `--deep`, shown with `--deep` (quickstart Q6, SC-007).

### Implementation for User Story 6

- [X] T069 [US6] Convert `RulePackCategory` into `CleanCategory`, expanding `%LOCALAPPDATA%`-style env vars in roots, in src/services/exclusion_service.rs (FR-015)
- [X] T070 [US6] Merge rule-pack categories into discovery in src/infra/win/clean_paths.rs (FR-015)
- [X] T071 [P] [US6] Merge rule-pack categories into discovery in src/infra/linux/clean_paths.rs (FR-015, Principle V)
- [X] T072 [US6] Apply the built-in visibility policy to custom categories — `risk = "System"` hidden unless `--deep` — in src/services/clean_service.rs (FR-016)
- [X] T073 [US6] Skip a rule-pack category whose id collides with a built-in (warning) and skip missing roots silently in src/infra/rulepack.rs (data-model.md validation, edge case)
- [X] T074 [US6] Add the `--rules <path>` flag to src/ui/cli.rs and load the additional pack alongside `sweep.toml` in `load_policy()` in src/services/exclusion_service.rs (research R9)
- [X] T075 [US6] Log a clear error and fall back to built-in categories when a rule-pack path is invalid in src/infra/rulepack.rs (FR-017, US6 acceptance #3)
- [X] T076 [P] [US6] Add unit tests in src/infra/rulepack.rs covering a valid `[[category]]`, an unknown `risk` value, an empty id, and an unreadable path (FR-015, FR-017)
- [X] T077 [P] [US6] Add doc comments to the rule-pack merge and conversion functions in src/services/exclusion_service.rs (FR-021)

**Checkpoint**: User Stories 1-6 all independently functional

---

## Phase 9: User Story 7 - Distribution Polish (Priority: P7)

**Goal**: Smaller tuned release binary, `sweep --version` with an online update hint, and winget/scoop manifests plus the Linux artifact on tagged releases.

**Independent Test**: `cargo build --release` produces a binary smaller than the ~7.8 MB untuned baseline; `cargo run -- --version` prints the version and, when online, an `update available: <tag>` hint (quickstart Q7, SC-008, SC-009).

### Implementation for User Story 7

- [X] T078 [US7] Add `[profile.release]` with `strip = true`, `lto = true`, `codegen-units = 1`, `opt-level = "z"` in Cargo.toml (FR-018, research R10)
- [X] T079 [US7] Attach the `sweep-linux-x64` artifact on `v*` tags in .github/workflows/release.yml (FR-020, SC-009)
- [X] T080 [US7] Implement the update check in src/services/system_service.rs — query the GitHub Releases API with a 2s timeout and a `User-Agent`, comparing `tag_name` to `CARGO_PKG_VERSION` (FR-019, research R3)
- [X] T081 [US7] Print `sweep <version>` plus `update available: <tag>` when newer, and version-only on offline/timeout/non-2xx, wired into the `--version` path in src/main.rs (FR-019, edge case)
- [X] T082 [P] [US7] Add winget manifest generation on `v*` tags in .github/workflows/release.yml (FR-020, SC-009)
- [X] T083 [P] [US7] Add scoop manifest generation on `v*` tags in .github/workflows/release.yml (FR-020, SC-009)
- [X] T084 [P] [US7] Add doc comments to the version-check code in src/services/system_service.rs (FR-021)

**Checkpoint**: All seven user stories complete

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Verification, documentation, and release readiness across all stories

- [X] T085 [P] Run `cargo build --release` and record the binary size against the ~7.8 MB baseline (SC-008)
- [X] T086 [P] Run `cargo test --locked` and confirm green on both `ubuntu-latest` and `windows-latest` per .github/workflows/ci.yml (SC-010) — 97 tests green locally on Windows (95 lib + 2 integration, 1 live probe ignored). Ubuntu leg still needs CI to confirm: no Linux toolchain is available on this host
- [X] T087 [P] Audit every new public item across src/domain, src/infra, src/services, and src/ui for doc comments (FR-021, Principle VII)
- [X] T088 [P] Document the new commands and the `sweep.toml` schema (doctor, undo, idle `--close`/`--kill --force`, bg, `--version`, `--config`, `--rules`) in README.md
- [X] T089 [P] Document the Recycle-Bin-purge undo limitation in README.md (spec edge case, Principle II)
- [X] T090 [P] Mark Stage 3 complete in ROADMAP.md
- [X] T091 [P] Bump `version` in Cargo.toml for the Stage 3 release
- [ ] T092 Execute quickstart.md Q1-Q7 end-to-end on Windows and confirm the stable output fields match specs/003-trust-control/contracts/cli.md — **partially done**:
  - [X] Q1 doctor — all contract fields printed, exit 0, 3.0-4.3 s (SC-001)
  - [X] Q2 exclusions — `dev-pnpm` absent from `clean --scan-only` and `diagnose` with `excluded: 1`; reappears when `sweep.toml` is removed (SC-002)
  - [X] Q3 undo — `clean -y` then `undo` restored both items to their original paths (SC-003). Purged-Bin branch NOT run live: it would require emptying the user's real Recycle Bin; covered by unit tests and the rendering was observed during debugging
  - [~] Q4 kill — `--kill` without `--force` refused, blocklist unit-tested, no-match path verified. NOT run live: no real process was closed or killed, and guard `--allow-kill` was not exercised against a real >500 MB/h offender (SC-004, SC-005 unverified)
  - [ ] Q5 TUI — `b`/`i`/`k` implemented but not exercised; needs an interactive terminal session (SC-006 unverified)
  - [X] Q6 rule packs — custom `myapp-cache` sized at 32.00 B with `Safe`; hidden without `--deep` and shown with it when `risk = "System"` (SC-007)
  - [X] Q7 distribution — release binary 2.45 MiB vs ~7.8 MB baseline; `sweep --version` printed `update available: v0.8.0` (SC-008). Tagged-release manifest generation NOT run (SC-009 unverified until a `v*` tag is pushed)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — complete
- **Foundational (Phase 2)**: Depends on Setup — complete; BLOCKS all user stories
- **User Stories (Phases 3-9)**: All depend on Phase 2 only. They may run in parallel, or sequentially in priority order P1 → P7
- **Polish (Phase 10)**: Depends on all desired user stories

### User Story Dependencies

- **US1 (P1, Doctor)**: Foundational only. Consumes `infra/{win,linux}/doctor.rs`, `exclusion_service`, `idle_service`
- **US2 (P2, Exclusions)**: Foundational only. Consumes `exclusion_service::load_policy` (T013)
- **US3 (P3, Undo)**: Foundational only. Consumes `infra/undo.rs` (T011) + `undo_service` (T019)
- **US4 (P4, Kill)**: Foundational only. Consumes `kill_service` (T014) + `process_lock` close/kill (T015, T016)
- **US5 (P5, TUI)**: Foundational only, but T064 shares `KillService` with US4 — land US4 first if both are in flight, to avoid duplicating the confirm path
- **US6 (P6, Rule packs)**: Foundational only. Consumes `rulepack.rs` (T010). T074 touches the same `--config` plumbing as T038 (US2) — coordinate that one file
- **US7 (P7, Distribution)**: Fully independent of every other story; can be done at any time

### Within Each User Story

- Services before UI before routing (Principle III: domain → services → infra → ui)
- Core implementation before the tests marked [P]
- Story complete and validated before moving to the next priority

### Parallel Opportunities

- Phases 1 and 2 are done, so all seven stories are unblocked right now
- Within US1: T027 (ui/doctor.rs), T030 (test), T031 (docs) run parallel to the service work
- Within US2: T039 and T040 are different files; T036 touches two UI files independent of the service edits
- Within US4: T058 (blocklist tests) runs parallel to the CLI routing tasks
- Within US6: T070 (win) and T071 (linux) are separate files
- Within US7: T082 and T083 are separate workflow steps; T080/T081 are independent of both
- Phase 10: T085-T091 are all parallel; T092 runs last

---

## Parallel Example: User Story 1 (Doctor)

```bash
# After T021-T026 (doctor_service.rs) lands, run these together:
Task: "T027 Create src/ui/doctor.rs renderer per contracts/cli.md"
Task: "T030 Add would_clean_total_bytes sum test in src/services/doctor_service.rs"
Task: "T031 Add doc comments to doctor_service.rs and ui/doctor.rs"
```

## Parallel Example: Cross-Story (post-Foundational)

```bash
# Seven independent tracks, one per story:
Track A: T021-T031  (US1 Doctor)
Track B: T032-T040  (US2 Exclusions)
Track C: T041-T047  (US3 Undo)
Track D: T048-T059  (US4 Kill)
Track E: T060-T068  (US5 TUI — after US4 if sharing KillService)
Track F: T069-T077  (US6 Rule packs)
Track G: T080-T084  (US7 Distribution)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phases 1 and 2 are already complete
2. Complete Phase 3: User Story 1 (T021-T031)
3. **STOP and VALIDATE**: run quickstart Q1 — `cargo run -- doctor` prints all stable fields, exits 0, under 5s
4. Demo: a user can now verify guard safety before walking away

### Incremental Delivery

1. Foundation ready (Phases 1-2) ✅
2. Add US1 (Doctor) → validate Q1 → demo (MVP)
3. Add US2 (Exclusions) → validate Q2 → demo
4. Add US3 (Undo) → validate Q3 → demo
5. Add US4 (Kill) → validate Q4 → demo
6. Add US5 (TUI) → validate Q5 → demo
7. Add US6 (Rule packs) → validate Q6 → demo
8. Add US7 (Distribution) → validate Q7 → ship
9. Each story adds value without breaking the previous ones

### Parallel Team Strategy

Foundation is done, so all seven stories can start immediately. With limited capacity, prioritize US1 → US2 → US3 (the trust triad: verify, exclude, recover) before the termination and distribution work.

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps each task to a user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate a story in isolation
- Constitution Principle II: every termination path is blocklist-guarded and consent-gated; guard is graceful-close-only
- Constitution Principle III: `ui/` parses and formats only — orchestration lives in `services/`, OS calls in `infra/`
- Constitution Principle V: every Windows infra addition needs a Linux counterpart
- Constitution Principle VII: FR-021 requires doc comments on all new public items
