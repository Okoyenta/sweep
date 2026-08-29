# Feature Specification: Immediate Relief & Visibility (Stage 1)

**Feature Branch**: `001-immediate-relief-visibility`

**Created**: 2026-08-26

**Status**: Draft

**Input**: User description: "Stage 1 — Immediate Relief & Visibility (1–2 days) — fixes F1+F5+F2+F7 (partial) Goal: never get bricked at 0 B again; see what's hogging now; reclaim dev bloat instantly. No elevation, no service stops, no kills beyond trim (constitution II-tier 1). Delivers: F1 Space Reserve 512 MB (SPACE_RESERVE.md:12): src/infra/paths.rs:1 reserve_path() / ensure_reserve() / consume_reserve() / is_disk_full_error(), src/main.rs:53 open_store_with_reserve() + run_status:492 fallback (show RAM/disks/top even if index unavailable), run_clean:247 ensure_headroom_or_consume_reserve() (<256 MB) + fix empty-only bug (src/main.rs:235 Some(&only)→None when empty, CleanService::run :54). Re-create reserve after clean/bin --empty. SWEEP_DB env fallback to D:. F5 Dev caches scan: src/infra/dev_caches.rs (new, shared win+linux) discovers pnpm (%LOCALAPPDATA%/pnpm/store), cargo (~/.cargo/registry/cache + git/checkouts), gradle (~/.gradle/caches), uv/pipx — added to discover_categories() (win/clean_paths.rs:17 / linux/clean_paths.rs:81), tests like linux/clean_paths.rs:92. F2+F3 Detection skeleton: wire sysinfo 0.39.6 Process::disk_usage() into SystemSnapshot (src/domain/models.rs:22 add read_bytes/write_bytes/total_written, src/infra/sysinfo_monitor.rs:69 call disk_usage()), src/infra/win/idle.rs WinIdleProbe (GetLastInputInfo + GetForegroundWindow via windows-sys Win32_UI_Input flag) — Linux stub heuristic. sweep diagnose (new src/ui/diagnose.rs) prints Category | Size | Risk | Reclaim sorted, including new dev cats + potential reclaim rollup (Optimizer 01). F7 Benchmark stub: sweep clean/bin now snapshots SysinfoMonitor before/after and prints before X free → after Y free (freed Z) in Ns (src/ui/clean.rs:27). Ship check: sweep diagnose, sweep clean --scan-only shows pnpm/cargo/gradle sizes, sweep status works at 0 B by consuming reserve.bin, cargo test --locked green both OS. Fixes together: F1 unlocks you to run F5 reclaims; F2/F3 visibility makes next stage's auto-actions trustworthy."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Survive 0 B Disk Full and Keep Diagnostics Usable (Priority: P1)

User's system drive `C:` has filled to `0 B` free (live case: 0.00 B, 7.97 GiB reclaimable but `sweep status` fails with SQLite disk I/O error 1546, `cargo build` fails). User needs sweep itself to remain operable so they can diagnose and reclaim space without manual registry/file hunting. The system pre-reserves 512 MB at `%LOCALAPPDATA%\sweep\reserve.bin` (Linux: `~/.local/share/sweep/reserve.bin`), automatically consumes it when free space falls below critical thresholds, and falls back to showing live system health even if the index DB cannot be opened.

**Why this priority**: Without it, all other cleaners are unreachable — the tool bricks exactly when needed most (F1). This is the unlock for every reclaim action.

**Independent Test**: Fill a test volume to <256 MB free (or mock free-space probe), verify `sweep status` still prints RAM/disks/top-processes after reserve consumption instead of hard-failing, and `sweep clean` can complete a trash operation. Verify `sweep index` / `sweep status` re-creates reserve after successful `sweep clean` + `sweep bin --empty`.

**Acceptance Scenarios**:

1. **Given** reserve.bin (512 MB) exists and disk has 0 B free, **When** user runs `sweep status`, **Then** system auto-consumes reserve.bin to free ~512 MB, opens or bypasses index gracefully, and prints RAM, disk free, and top processes with a notice "index unavailable (reserve consumed — run sweep bin --empty)".
2. **Given** free space is 200 MB (<256 MB) before clean, **When** user runs `sweep clean -y`, **Then** system consumes reserve first to gain headroom for trash metadata and proceeds with clean instead of failing with disk-full.
3. **Given** user has a second volume `D:` and sets `SWEEP_DB=D:\sweep\index.db`, **When** `C:` is full, **Then** status/index operations use the alternate DB path and succeed without requiring reserve consumption on `C:`.
4. **Given** `sweep clean` was filtered with `--only` but no categories matched (empty `only` list bug), **When** user runs `sweep clean --scan-only` with no filter or with empty filter, **Then** system scans all categories (does not silently skip everything).

---

### User Story 2 - See and Reclaim Dev Cache Bloat Instantly (Priority: P1)

Developer notices disk disappearing to toolchain caches (pnpm store, Cargo registry/cache + git checkouts, Gradle caches, uv/pipx). They want a single scan that surfaces these alongside existing categories (npm, pip, user-temp, browser caches) with size and risk, and lets them reclaim with `sweep clean`.

**Why this priority**: Live scan showed `npm-cache 6.64 GiB` and 7.97 GiB total reclaimable; dev caches are the fastest safe win and share the same trash-backed safety model as existing cleaners. F5 directly funds F1 recovery.

**Independent Test**: On Windows and Linux with seeded cache directories, `sweep diagnose` and `sweep clean --scan-only` list `pnpm`, `cargo`, `gradle`, `uv`/`pipx` with non-zero sizes that sum into total potential reclaim. Cleaning one category moves it to Recycle Bin/trash and updates free-space reporting.

**Acceptance Scenarios**:

1. **Given** pnpm store exists at `%LOCALAPPDATA%\pnpm\store` with 1.2 GiB, **When** user runs `sweep clean --scan-only`, **Then** output includes a `pnpm` row with ~1.2 GiB, risk `Safe`, and it is included in the total reclaimable sum.
2. **Given** cargo caches exist (`~/.cargo/registry/cache` + `~/.cargo/git/checkouts`), gradle caches (`~/.gradle/caches`), uv/pipx caches, **When** user runs `sweep diagnose`, **Then** each present category appears sorted by size descending with Risk and Reclaim columns.
3. **Given** user runs `sweep clean --only cargo -y`, **When** operation completes, **Then** only cargo-related paths are trashed, other categories remain untouched, and reserve is re-created afterwards if previously consumed.

---

### User Story 3 - Understand What Is Hogging Disk/RAM Right Now (Priority: P2)

User suspects background or idle apps are writing heavily to SSD or holding RAM (F2/F3). They need a read-only diagnostic view that shows per-process disk I/O, per-category reclaim potential, and idle-state hints, without any automatic killing or service stops.

**Why this priority**: Visibility is prerequisite for trust in Stage 2 automation. Skeleton detection must be correct before auto-actions are enabled.

**Independent Test**: Run `sweep diagnose` on a system with known I/O-heavy processes; verify output table `Category | Size | Risk | Reclaim` is sorted by size, includes a rollup "potential reclaim: X (Safe Y + ...)", and that system snapshot data includes per-process `read_bytes/write_bytes/total_written` from the underlying monitor.

**Acceptance Scenarios**:

1. **Given** system has multiple clean categories with varying sizes, **When** user runs `sweep diagnose`, **Then** output prints a sorted table `Category | Size | Risk | Reclaim` and a summary line `potential reclaim: <total>` covering both existing and new dev categories.
2. **Given** a process has performed heavy disk writes, **When** diagnose/status collects `SystemSnapshot`, **Then** snapshot exposes `read_bytes`, `write_bytes`, `total_written_bytes` per process (sourced from OS process disk-usage probe).
3. **Given** Windows session has been idle (no input, foreground window unchanged), **When** idle probe is queried, **Then** it reports idle seconds via `GetLastInputInfo` and foreground window via `GetForegroundWindow`; on Linux it returns a stub/heuristic value without crashing.
4. **Given** no elevation and constitution tier-1 constraint, **When** diagnose runs, **Then** it performs zero process termination, service stops, or privileged operations — read-only.

---

### User Story 4 - See Before/After Benefit of Any Clean (Priority: P3)

User wants immediate feedback that cleaning helped and how long it took, to decide whether to empty the bin or run deeper cleans later.

**Why this priority**: F7 benchmark stub builds trust with minimal cost; required to prove F1+F5 fix actually freed space.

**Independent Test**: Run `sweep clean -y` and `sweep bin --empty -y` and verify each prints `before X free → after Y free (freed Z) in Ns` where X/Y are free bytes on the index volume and N is elapsed seconds.

**Acceptance Scenarios**:

1. **Given** volume has 10 GiB free before clean, **When** user runs `sweep clean -y` that frees 2 GiB, **Then** output includes `before 10.0 GiB free → after 12.0 GiB free (freed 2.0 GiB) in <N>s`.
2. **Given** user runs `sweep bin --empty -y`, **When** purge completes, **Then** same before/after line is printed for the bin operation.

---

### Edge Cases

- Reserve missing at 0 B (user never ran `sweep index`/`status` before fill): `sweep status` cannot auto-recover; document manual recovery (`sweep clean --only npm-cache -y` + `sweep bin --empty -y` equivalent) and surface actionable hint.
- Reserve file locked by AV or concurrent sweep instance: consume/re-create must be idempotent and not crash; second attempt reports `reserve: missing/locked` gracefully.
- `free_bytes_on_index_volume` reporting stale or error: treat as 0 and attempt reserve consumption conservatively rather than failing the clean.
- Empty `--only` list regression: `Some(&only)` with empty vec must be coerced to `None` (clean all) — verify with unit test `CleanService::run` with empty filter cleans all, not zero.
- `SWEEP_DB` path on non-existent `D:` drive: error message must name the missing path and suggest fallback to default.
- Cross-platform: `dev_caches.rs` discovery skips missing roots without error; Linux `clean_paths.rs` and Windows `clean_paths.rs` both include dev categories and tests pass on both OS in CI (`cargo test --locked` green).
- Disk I/O counters overflow or are unavailable (sysinfo returns 0): snapshot records 0 without breaking status/diagnose output.
- Diagnose with no reclaimable categories: prints empty table with `potential reclaim: 0 B` rather than error.
- Benchmark timing: elapsed time measured around actual clean/bin operation only, not including scan.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST pre-reserve 512 MB at a well-known path (`%LOCALAPPDATA%\sweep\reserve.bin` on Windows, `~/.local/share/sweep/reserve.bin` on Linux) via creation of a sparse file of exact size on first `sweep index` or `sweep status` if not present.
- **FR-002**: System MUST provide helpers to `ensure_reserve`, `consume_reserve` (delete file and return freed bytes), `reserve_path`, and `is_disk_full_error` detection for SQLite I/O errors (code 1546 and generic "No space left"/"disk I/O error" matching).
- **FR-003**: System MUST attempt to open the index DB via `open_store_with_reserve`: on disk-full error, consume reserve and retry once; if still failing, `sweep status` MUST NOT hard-fail but instead print live RAM, disk, and top-process info with notice `index: unavailable (disk full, reserve consumed — run sweep bin --empty)`.
- **FR-004**: System MUST ensure headroom before trash operations: if `free_bytes_on_index_volume < 256 MB`, `sweep clean` and `sweep bin` MUST consume reserve before invoking trash metadata writes.
- **FR-005**: System MUST re-create reserve after successful `sweep clean` and after `sweep bin --empty` when free space permits.
- **FR-006**: System MUST honor `SWEEP_DB` environment variable (and `--db-path` if present) to relocate index DB to an alternate volume (e.g., `D:\sweep\index.db`); `index_db_path()` MUST fallback to `D:` when `C:` is chronically full and env is set.
- **FR-007**: System MUST fix the empty-only bug: `CleanService::run(&scans, Some(&only))` where `only` is empty MUST behave as `None` (clean all scanned categories), not skip all.
- **FR-008**: System MUST discover dev cache categories via a shared module (`dev_caches.rs`): `pnpm` (`%LOCALAPPDATA%/pnpm/store`), `cargo` (`~/.cargo/registry/cache`, `~/.cargo/registry/src`, `~/.cargo/git/checkouts`), `gradle` (`~/.gradle/caches`), `uv`/`pipx` (`~/.local/share/uv`, `~/.local/share/pipx` or platform equivalent) — each as a `CleanCategory` with `risk=Safe`, size via directory walk.
- **FR-009**: System MUST include dev categories in `discover_categories()` on both Windows (`win/clean_paths.rs`) and Linux (`linux/clean_paths.rs`) and include tests analogous to `linux/clean_paths.rs:92` for discovery.
- **FR-010**: System MUST extend `SystemSnapshot` / process model to include `read_bytes`, `write_bytes`, `total_written_bytes` per process, populated via `sysinfo 0.39.6` `Process::disk_usage()`.
- **FR-011**: System MUST provide `WinIdleProbe` on Windows using `GetLastInputInfo` + `GetForegroundWindow` via `windows-sys` with `Win32_UI_Input` feature; on Linux MUST provide a stub/heuristic implementation that does not require elevation.
- **FR-012**: System MUST add `sweep diagnose` command that prints a sorted table `Category | Size | Risk | Reclaim` including all existing + new dev categories and a rollup line `potential reclaim: <total>` (sum of reclaimable bytes).
- **FR-013**: System MUST show benchmark before/after for `sweep clean` and `sweep bin`: snapshot free bytes via `SysinfoMonitor` before and after operation and print `before X free → after Y free (freed Z) in Ns`.
- **FR-014**: System MUST NOT require elevation, service stops, or process kills beyond working-set trim for any Stage 1 operation (constitution II-tier 1 only).
- **FR-015**: System MUST keep `cargo test --locked` green on both `windows-latest` and `ubuntu-latest` CI.

### Key Entities

- **SpaceReserve**: File at `reserve_path` of fixed size 512 MB (sparse). Attributes: exists, size_bytes, consumed flag. Lifecycle: ensured on first index/status, consumed when free < thresholds, re-created after reclaim.
- **DiskHeadroom**: Free bytes on index volume as reported by system monitor. Thresholds: `<256 MB` triggers reserve consumption before clean; `<2 GB` is guard threshold (informational in Stage 1).
- **CleanCategory**: Whitelisted reclaimable location. Attributes: id (e.g., `pnpm`, `cargo`, `gradle`), roots (one or more paths), discovered size_bytes, risk (`Safe` for Stage 1 dev caches), reclaimable flag. Includes dev_caches plus existing npm/pip/temp/browser caches.
- **SystemSnapshot**: Live system health at a point in time. Attributes: total/used RAM, disks with free/total, top processes (pid, name, rss, cpu, read_bytes, write_bytes, total_written_bytes), timestamp.
- **DiagnoseReport**: Derived view for `sweep diagnose`. Attributes: rows `Category | Size | Risk | Reclaim` sorted by size descending, total potential reclaim, per-category risk, benchmark data if applicable.
- **BenchmarkSample**: Before/after free-space snapshot for a clean/bin operation. Attributes: before_free_bytes, after_free_bytes, freed_bytes, elapsed_secs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: User with 0 B free on `C:` can run `sweep status` and see RAM/disks/top-processes within 2 seconds after reserve consumption, instead of receiving SQLite error 1546 — verified by forcing disk-full mock.
- **SC-002**: Reserve file is exactly 512 MB (±1 MB) and is re-created within 1 second after successful `sweep clean -y` + `sweep bin --empty -y` when free space ≥1 GB.
- **SC-003**: `sweep diagnose` and `sweep clean --scan-only` surface pnpm/cargo/gradle/uv sizes within 5 seconds on a typical dev machine, and total `potential reclaim` equals sum of displayed `Reclaim` column (tolerance ±1%).
- **SC-004**: `sweep clean` on a volume with <256 MB free completes trash of at least one Safe category without disk-full failure in 95% of trials (reserve-headroom path exercised).
- **SC-005**: Empty `--only` filter no longer skips all — `sweep clean --scan-only --only ""` equivalent or `CleanService::run` with empty vec cleans all categories (100% of regression tests pass).
- **SC-006**: SystemSnapshot exposes per-process `read_bytes/write_bytes` non-zero for at least one active process on Windows where sysinfo provides disk_usage, validated via live probe test.
- **SC-007**: Every `sweep clean` and `sweep bin --empty` invocation prints `before X free → after Y free (freed Z) in Ns` where Z = Y − X and N < operation wall time + 1s.
- **SC-008**: No Stage 1 command requires elevation or performs service stop/kill — verified by running full test suite as non-admin on both Windows and Linux with 0 failures related to privilege.
- **SC-009**: `cargo test --locked` passes on both `ubuntu-latest` and `windows-latest` CI for the Stage 1 branch (0 failed tests, ignored live probes allowed).

## Assumptions

- Reserve size 512 MB is the default (256 MB minimal, 1024 MB for users building on `C:`) per `SPACE_RESERVE.md:12`; user can tune via future `sweep reserve --size-mb` but Stage 1 ships with 512 MB fixed.
- Reserve lives alongside index DB at `%LOCALAPPDATA%\sweep\reserve.bin` (sparse file via `File::create` + `set_len`); Linux equivalent is `~/.local/share/sweep/reserve.bin`.
- Thresholds: consume reserve when free <256 MB before clean; `<2 GB` is informational guard threshold not auto-acting in Stage 1.
- `SWEEP_DB` env fallback to `D:` is best-effort — if `D:` does not exist, command fails with clear message rather than silently using `C:`.
- `sysinfo 0.39.6` is the locked version supplying `Process::disk_usage()`; API shape is `disk_usage().read_bytes / written_bytes / total_written_bytes`.
- `windows-sys 0.61.2` with `Win32_UI_Input` feature provides `GetLastInputInfo` and `GetForegroundWindow`; Linux idle is stub/heuristic returning 0 or time since last snapshot.
- Stage 1 does NOT implement guard daemon, deep system cats (WU/WinSxS), service-aware unlock, or kill paths — those remain Stage 2/3.
- All cleaners remain trash-backed (Recycle Bin / trash); `sweep bin --empty` is the only permanent delete in Stage 1.
- Cross-compile limitation (`x86_64-linux-gnu-gcc` missing locally) is accepted; Linux correctness is gated by CI.
- TUI changes are out of scope for Stage 1 except benchmark display if trivial.
