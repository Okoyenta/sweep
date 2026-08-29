# Feature Specification: Guard Daemon & Deep System Cleaning

**Feature Branch**: `002-guard-deep-clean`

**Created**: 2026-08-26

**Status**: Draft

**Input**: User description: "Stage 2 — Safe Automation & Deep Cleaning (3–5 days) — fixes F4+F6+F3+F7 (full) + F2 auto. Goal: sweep guard actually prevents hangs + deep system bloat, with service-aware unlock but still trim-only by default. System cats stay scan-only unless --deep."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Guard Prevents System Hang (Priority: P1)

As a user running sweep guard in the background, I want the system to automatically reclaim RAM and disk space before the machine becomes unresponsive, so I never experience a hard hang due to resource exhaustion.

**Why this priority**: This is the core value proposition — preventing data loss from hangs. Without this, deep cleaning and idle detection are moot.

**Independent Test**: Can be fully tested by running `sweep guard --once` with simulated high RAM (≥90%) and verifying that trim actions occur, free space increases, and a log entry is written.

**Acceptance Scenarios**:

1. **Given** system RAM usage is ≥90% for 3 consecutive 30-second samples, **When** guard runs its polling loop, **Then** guard trims the top-10 working sets and purges standby memory, logging bytes freed.
2. **Given** system free disk is <2 GB, **When** guard runs its polling loop, **Then** guard consumes reserve space, trashes safe categories (user temp, browser caches, npm/pip), and if still low, purges the Recycle Bin — all logged.
3. **Given** guard has just performed a rescue action, **When** less than 10 minutes have elapsed, **Then** guard enters cooldown and skips further action until the cooldown expires.
4. **Given** guard is running, **When** RAM or disk pressure triggers action, **Then** a Windows toast notification is displayed with the action summary (graceful no-op if toast unavailable).
5. **Given** guard is running with default settings, **When** no pressure is detected, **Then** guard polls silently with near-zero CPU usage.

**Independent Test**: Can be tested by running `sweep guard --once` on a system with high RAM and checking `guard.log` for trim entries and toast output.

---

### User Story 2 — Deep System Scan Discovers Hidden Bloat (Priority: P2)

As a power user running `sweep diagnose --deep`, I want sweep to discover Windows Update downloads, Delivery Optimization caches, WinSxS component store bloat, and stale drivers, so I can see the true reclaimable space before deciding to clean.

**Why this priority**: Deep scan visibility is prerequisite to safe deep cleaning — users must see what exists before they can opt into cleaning it.

**Independent Test**: Can be tested by running `sweep diagnose --deep` and verifying that WU, DO, WinSxS, and driver store entries appear in the output with accurate size estimates.

**Acceptance Scenarios**:

1. **Given** a Windows system with accumulated Windows Update downloads, **When** user runs `sweep diagnose --deep`, **Then** the output includes `SoftwareDistribution\Download` size and is marked with risk level "System".
2. **Given** a Windows system with Delivery Optimization files, **When** user runs `sweep diagnose --deep`, **Then** the output includes DO cache size with risk level "System".
3. **Given** a Windows system, **When** user runs `sweep diagnose --deep`, **Then** WinSxS component store analysis is performed via a read-only system assessment (no deletion), and the estimated reclaimable space is displayed.
4. **Given** a Windows system with stale drivers, **When** user runs `sweep diagnose --deep`, **Then** driver store age and size are reported.
5. **Given** user runs `sweep diagnose` without `--deep`, **When** the command completes, **Then** system-level categories (WU, DO, WinSxS, drivers) are NOT shown in output.
6. **Given** user runs `sweep clean --scan-only --deep`, **When** the command completes, **Then** no files are deleted, only sizes are estimated.

**Independent Test**: Can be tested by running `sweep diagnose --deep` on a Windows system and verifying system bloat categories appear.

---

### User Story 3 — Service-Aware Unlock for Safe Deep Cleaning (Priority: P3)

As a power user running `sweep clean --deep --stop-services`, I want sweep to temporarily stop Windows Update and BITS services, clean the SoftwareDistribution\Download folder, then restore the services, so I can reclaim space that is normally locked.

**Why this priority**: Service-aware unlock enables cleaning categories that are otherwise inaccessible — it's the mechanism that makes deep cleaning effective.

**Independent Test**: Can be tested by running `sweep clean --deep --stop-services -y` and verifying that wuauserv and bits services are stopped before cleaning and restored after.

**Acceptance Scenarios**:

1. **Given** user runs `sweep clean --deep --stop-services -y`, **When** services are running, **Then** wuauserv and bits are stopped, SoftwareDistribution\Download is trashed, and both services are restored regardless of success or failure.
2. **Given** sweep has stopped services for cleaning, **When** the clean operation completes or fails, **Then** services are always restored (guaranteed via RAII/drop semantics).
3. **Given** sweep guard is running in background, **When** guard detects disk pressure, **Then** guard NEVER auto-stops services (service stop requires explicit `--stop-services` flag or `--allow-service-stop` on the guard command).

**Independent Test**: Can be tested by checking that guard does not stop services even under disk pressure.

---

### User Story 4 — Idle SSD Offender Detection (Priority: P4)

As a user running `sweep idle`, I want to see which background processes are writing heavily to disk while idle, so I can identify cache bloat, log spam, or misbehaving apps consuming SSD endurance.

**Why this priority**: Idle SSD detection protects hardware longevity and identifies hidden resource waste — valuable but secondary to preventing hangs.

**Independent Test**: Can be tested by running `sweep idle` and verifying the output table includes PID, app name, idle duration, write rate, RAM usage, and a reason code.

**Acceptance Scenarios**:

1. **Given** a background process has been idle for >30 minutes and is writing >100 MB/hour, **When** user runs `sweep idle`, **Then** the process appears in the output table with columns: PID, APP, IDLE duration, WRITE/h, RAM, REASON.
2. **Given** user runs `sweep idle --top 5 --idle-mins 60 --min-write-mb 200`, **When** the command completes, **Then** only processes matching the specified thresholds are shown, limited to 5 results.
3. **Given** an idle offender is identified, **When** user runs `sweep idle --clean-cache`, **Then** whitelisted cache data for that process is cleaned using the same safe categories as regular clean.
4. **Given** the foreground application is writing heavily, **When** sweep idle scans, **Then** the foreground app is excluded from the offender list (it's not idle).

**Independent Test**: Can be tested by running `sweep idle` on a system with known idle heavy writers and verifying they appear in the output.

---

### User Story 5 — Full Benchmark Visibility (Priority: P5)

As a user, I want every clean and guard operation to log before/after free space and per-category bytes removed, and I want `sweep diagnose` to show total reclaimable space split by Safe and System categories, so I can verify that sweep actually reclaims space.

**Why this priority**: Benchmarking builds trust and proves the tool works — it's the feedback loop that validates all other features.

**Independent Test**: Can be tested by running `sweep clean` and verifying the output includes before/after free space and per-category removal counts.

**Acceptance Scenarios**:

1. **Given** user runs `sweep clean`, **When** the clean completes, **Then** output includes total free space before and after, and a per-category breakdown of bytes removed.
2. **Given** user runs `sweep guard --once` and it triggers a trim, **When** the trim completes, **Then** `guard.log` includes before/after free space and per-category bytes removed.
3. **Given** user runs `sweep diagnose --deep`, **When** the command completes, **Then** output shows total reclaimable space split into "Safe" (user-cache categories) and "System" (WU, DO, WinSxS, drivers) amounts.

**Independent Test**: Can be tested by running `sweep diagnose --deep` and verifying Safe/System split appears.

---

### User Story 6 — Guard Autostart via Schedule (Priority: P6)

As a user, I want to enable guard autostart via `sweep schedule` so that guard runs automatically on every logon without manual configuration.

**Why this priority**: Autostart makes guard a set-and-forget solution, but users can start guard manually first.

**Independent Test**: Can be tested by running `sweep schedule` and verifying a logon task is registered.

**Acceptance Scenarios**:

1. **Given** user runs `sweep schedule` to enable autostart, **When** the command completes, **Then** a Windows scheduled task (schtasks /SC ONLOGON) is registered pointing at `sweep guard`.
2. **Given** autostart is enabled, **When** user logs in, **Then** `sweep guard` starts automatically as a background process.

**Independent Test**: Can be tested by checking the scheduled task exists after running `sweep schedule`.

---

### Edge Cases

- What happens when guard is already running and user tries to start a second instance? — Single-instance mutex prevents duplicate guards; second instance exits with informative message.
- What happens when toast notifications are unavailable (e.g., server Core, headless)? — Guard logs the notification attempt and continues silently; no error.
- What happens when stopping a service fails? — ServiceGuard restores any services it did manage to stop; logs the failure; continues cleanup of accessible files.
- What happens when dism analysis is unavailable or requires elevation? — Deep scan reports "access denied" for WinSxS and continues with other categories.
- What happens when the user runs `sweep clean --deep` without `--stop-services`? — System categories that require service stop are skipped; only safe categories are cleaned.
- What happens when guard's disk rescue frees space above the threshold? — Guard exits the disk branch early without purging Recycle Bin.
- What happens when an idle process exits between snapshot and comparison? — Snapshot diff handles missing PIDs gracefully; they are excluded from results.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Guard daemon MUST poll system resources every 30 seconds (configurable via `--interval-secs`) and perform near-zero work when no pressure is detected.
- **FR-002**: Guard MUST use 3-sample RAM hysteresis (3 consecutive polls ≥ threshold) before triggering trim actions, preventing false positives from transient spikes.
- **FR-003**: Guard MUST trim the top-10 working sets and purge standby memory when RAM pressure is confirmed, logging PID, name, and bytes freed for each trim.
- **FR-004**: Guard MUST detect disk pressure (free < 2 GB configurable via `--disk-min-gb`) and execute a graduated response: consume reserve → trash safe categories → purge Recycle Bin.
- **FR-005**: Guard MUST enforce a 10-minute cooldown between rescue actions to prevent notification and action spam.
- **FR-006**: Guard MUST log all actions to `%LOCALAPPDATA%\sweep\guard.log` with timestamps, action type, bytes freed, and affected items.
- **FR-007**: Guard MUST display Windows toast notifications via PowerShell WinRT for each rescue action, with graceful no-op on systems without toast support.
- **FR-008**: Guard MUST support a `--once` flag for single-pass execution (useful for testing and scripting).
- **FR-009**: Guard MUST use a single-instance mutex to prevent multiple guard processes from running simultaneously.
- **FR-010**: Deep scan MUST discover Windows Update downloads (`SoftwareDistribution\Download`), Delivery Optimization caches, WinSxS component store analysis (read-only via system assessment tool), and stale driver store entries.
- **FR-011**: Deep scan categories MUST be marked with risk level "System" and hidden from output unless `--deep` flag is provided.
- **FR-012**: WinSxS analysis MUST be read-only — the system assessment tool reads component store state without deleting anything.
- **FR-013**: Service-aware unlock MUST provide RAII/drop-based service management: stop services before cleaning, restore them in all code paths (success, failure, panic).
- **FR-014**: Service-aware unlock MUST default to stopping wuauserv and bits services; guard MUST NOT auto-stop services unless `--allow-service-stop` is explicitly set.
- **FR-015**: Idle SSD detection MUST compare two I/O snapshots taken 60 seconds apart, identifying processes with idle time >30 minutes and write rate >100 MB/hour that are not the foreground application.
- **FR-016**: Idle SSD output MUST display a table with columns: PID, APP, IDLE duration, WRITE/h, RAM, and REASON (e.g., CacheBloat, LogSpam).
- **FR-017**: Idle cache cleaning MUST reuse the same whitelisted clean categories as regular clean operations.
- **FR-018**: Every clean and guard operation MUST log before/after free disk space and per-category bytes removed.
- **FR-019**: `sweep diagnose --deep` MUST show total reclaimable space split into "Safe" (user-cache categories) and "System" (WU, DO, WinSxS, drivers) amounts.
- **FR-020**: Guard autostart MUST be configurable via `sweep schedule` using a logon scheduled task variant.
- **FR-021**: `sweep clean --scan-only --deep` MUST report sizes of system categories without deleting anything.
- **FR-022**: Guard MUST never auto-kill processes; it only trims working sets unless `--allow-kill` is explicitly set by the user.

### Key Entities

- **GuardConfig**: Runtime configuration for the guard daemon — RAM threshold, disk minimum, poll interval, cooldown duration, service-stop permission.
- **RamSnapshot**: Point-in-time snapshot of system memory state — total, used, available, standby bytes.
- **DiskSnapshot**: Point-in-time snapshot of disk free space per volume.
- **CleanCategory**: Classification of cleanable items with risk level (Safe or System) — user temp, browser cache, npm/pip, WU downloads, DO cache, WinSxS, drivers.
- **TrashBin**: Abstraction over Recycle Bin / trash operations — move items in, purge all, count items.
- **ServiceGuard**: RAII wrapper that stops specified Windows services on creation and restores them on drop.
- **IdleSsdOffender**: Identified process with high idle write activity — PID, name, idle duration, write rate, RAM usage, reason code.
- **BenchmarkRecord**: Before/after free space + per-category bytes removed for a clean or guard operation.
- **GuardLog**: Rolling log file at `%LOCALAPPDATA%\sweep\guard.log` — timestamped entries for every action.
- **ScheduledTask**: Windows logon task entry for guard autostart — task name, trigger, action path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `sweep guard --once` triggers trim actions when RAM ≥90% for 3 consecutive samples, with logged bytes freed >0.
- **SC-002**: Guard completes a full disk rescue cycle (reserve → safe categories → Recycle Bin) in under 60 seconds.
- **SC-003**: Guard never performs two rescue actions within 10 minutes of each other (cooldown enforced).
- **SC-004**: `sweep diagnose --deep` reports reclaimable space for WU, DO, WinSxS, and driver store categories on a typical Windows system with accumulated updates.
- **SC-005**: `sweep idle` identifies background processes writing >100 MB/hour while idle for >30 minutes, excluding the foreground app.
- **SC-006**: `sweep clean --deep --scan-only` reports system category sizes without deleting any files (0 bytes removed in System categories).
- **SC-007**: `sweep clean --deep --stop-services -y` stops wuauserv and bits, cleans SoftwareDistribution\Download, and restores both services — verified by service status check after completion.
- **SC-008**: Every clean/guard operation produces a benchmark log entry with before/after free space and per-category bytes removed.
- **SC-009**: Guard autostart registers a logon scheduled task that survives reboot.
- **SC-010**: Guard uses <50 MB RSS and <1% CPU while polling in healthy state.

## Assumptions

- Target platform is Windows 10/11; Linux support for deep system categories is out of scope for this stage (guard polling and idle detection work cross-platform).
- The system assessment tool for WinSxS analysis (`dism /Online /Cleanup-Image /AnalyzeComponentStore`) is available on Windows 10/11 Pro and Enterprise; Home editions may not have it.
- User has standard user permissions; elevation is required only for standby purge and service management (guard and deep clean degrade gracefully without elevation).
- The existing trash-backed cleaning infrastructure (`trash` crate) is reused for all category cleanup.
- Guard's single-instance mutex uses a file-based lock at `%LOCALAPPDATA%\sweep\guard.lock`.
- Toast notifications use PowerShell WinRT one-liner; systems without PowerShell 5.1+ or without WinRT support get no-op toast (logged).
- Service stop/restore uses Windows Service Control Manager via `windows-sys` FFI; guard logs service stop failures but continues with accessible files.
- Idle SSD detection uses the existing `sysinfo` crate for process I/O counters; snapshot interval is fixed at 60 seconds.
- Deep scan categories are hidden from non-deep output by checking the `--deep` flag in the UI layer, not by conditionally compiling the discovery code.
- Autostart uses `schtasks /SC ONLOGON /TN "sweep-guard" /TR "sweep guard"` — the task is registered for the current user only.
