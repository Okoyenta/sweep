# Feature Specification: Trust & Control (Stage 3)

**Feature Branch**: `003-trust-control`

**Created**: 2026-08-29

**Status**: Draft

**Input**: User description: "Stage 3 — Trust & Control (2–4 days) — fixes remaining F2+F3 control, undo, and distribution. Goal: user trusts unattended guard; can leave exclusions, undo, and kill only when they say so (constitution v1.1.0 tier 2/3). Delivers: Doctor + Exclusions, Undo journal, Controlled kill, Cleaner rule packs, Distribution polish."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Doctor Pre-Flight Builds Trust (Priority: P1)

As a user who is nervous about leaving sweep running unattended, I want a single `sweep doctor` command that reports the current safety state — elevation, toast availability, reserve file status, and exactly what guard *would* do right now — so I can decide whether to enable autostart with confidence.

**Why this priority**: Trust is the whole point of Stage 3. Without a pre-flight check, users cannot verify guard is armed correctly before walking away.

**Independent Test**: Can be fully tested by running `sweep doctor` on a healthy system and verifying it reports `reserve: ok`, `guard: armed`, elevation state, toast state, and a size estimate of what guard would trim/clean.

**Acceptance Scenarios**:

1. **Given** a system where sweep guard could run, **When** user runs `sweep doctor`, **Then** output reports reserve status (`ok` / `missing` / `consumed`), elevation state (`elevated` / `not`), and toast availability (`available` / `unavailable`).
2. **Given** guard is installed as a logon task, **When** user runs `sweep doctor`, **Then** output reports `guard: armed` and the categories it would clean plus their total size.
3. **Given** guard is not installed, **When** user runs `sweep doctor`, **Then** output reports `guard: not armed` and suggests the install command.
4. **Given** `sweep doctor` runs, **When** it completes, **Then** it also reports the number of currently detected idle offenders (e.g., `idle: 2 offenders`).

---

### User Story 2 — Per-User Exclusions via sweep.toml (Priority: P2)

As a user who has cache directories I never want touched (game installs, active project build dirs), I want to declare exclusions in a `sweep.toml` file, so sweep (diagnose, clean, and guard) leaves my stuff alone everywhere.

**Why this priority**: "Leave my stuff alone" is the single biggest blocker to trusting unattended cleaning. Exclusions must be honored by every code path.

**Independent Test**: Can be tested by adding an exclusion for a known cache dir to `sweep.toml`, then running `sweep diagnose`, `sweep clean --scan-only`, and `sweep guard --once` (dry) and verifying the excluded path never appears.

**Acceptance Scenarios**:

1. **Given** `sweep.toml` contains an excluded path or category id, **When** user runs `sweep diagnose` or `sweep clean --scan-only`, **Then** excluded items are omitted from the reclaimable list.
2. **Given** `sweep.toml` excludes a category id (e.g., `dev-pnpm`), **When** `sweep clean` or `sweep guard` runs, **Then** that category is skipped entirely and logged as excluded.
3. **Given** `sweep.toml` is absent or empty, **When** any sweep command runs, **Then** behavior is identical to today (no exclusions applied).
4. **Given** an exclude glob/pattern, **When** discovery walks the filesystem, **Then** matching roots are pruned before size calculation.

---

### User Story 3 — Undo Journal for Trashed Items (Priority: P3)

As a user who accidentally cleaned something useful, I want `sweep undo` to restore the items swept moved to the Recycle Bin in the most recent session, so a mistake is recoverable.

**Why this priority**: Recoverability is core to the Safety-First principle. Users will only trust cleaning if they know it can be undone.

**Independent Test**: Can be tested by running `sweep clean -y` on a safe category, then `sweep undo` and verifying the items reappear in their original locations.

**Acceptance Scenarios**:

1. **Given** `sweep clean` moved items to the Recycle Bin, **When** user runs `sweep undo`, **Then** the items from that session are restored to their original paths.
2. **Given** the Recycle Bin was purged (manually or by guard's auto-purge) after a clean, **When** user runs `sweep undo`, **Then** sweep reports that the items are unrecoverable and does not silently fail.
3. **Given** a session journal exists, **When** `sweep undo` runs, **Then** it restores only the most recent session's items (not historical sessions) and reports how many were restored.

---

### User Story 4 — Controlled Process Termination (Priority: P4)

As a user fighting idle apps that won't close, I want graduated, consent-gated termination: a graceful `WM_CLOSE` via `sweep idle --close`, and an explicit forced kill via `sweep idle --kill --force` (or `sweep bg --kill`) that requires a per-process confirmation prompt, so nothing is ever killed silently.

**Why this priority**: Constitution Principle II allows termination only with explicit consent and graduated escalation. This converts the kill capability (already wired in Stage 2) into a safe, user-controlled feature.

**Independent Test**: Can be tested by launching an idle test app, running `sweep idle --close` to verify graceful exit, then `sweep idle --kill --force` and confirming the confirmation prompt appears and logs consent.

**Acceptance Scenarios**:

1. **Given** an idle offender, **When** user runs `sweep idle --close`, **Then** sweep sends a graceful close (WM_CLOSE / non-forced taskkill) and the process exits cleanly.
2. **Given** an idle offender and `--kill --force`, **When** user runs the command, **Then** sweep prompts `confirm("kill <name> PID <pid> <size>?")` and only kills after approval, never for system-critical PIDs (PID 0/4, `csrss`, `wininit`, `services`).
3. **Given** a system-critical PID is among offenders, **When** sweep evaluates kill eligibility, **Then** it is always excluded by the blocklist regardless of flags.
4. **Given** guard runs with `--allow-kill`, **When** an idle offender writes `>500 MB/h` for `>60m`, **Then** guard performs a graceful close (tier 2) only — never a forced kill — and logs the consent/action.
5. **Given** guard runs without `--allow-kill`, **When** any idle offender is detected, **Then** guard never closes or kills it (trim-only, per Principle II).

---

### User Story 5 — TUI Background / Idle / Kill Views (Priority: P5)

As a user who prefers the interactive dashboard, I want the TUI to show a background-process view (`b`), an idle-offender view (`i`), and a kill action (`k`) with a confirmation modal, so I can manage processes without leaving the UI.

**Why this priority**: Brings the controlled-kill and idle-detection capabilities into the existing TUI surface, matching the CLI parity expected by users.

**Independent Test**: Can be tested by launching the TUI, pressing `b`/`i` to switch views, and `k` on a selected process to trigger the confirmation modal.

**Acceptance Scenarios**:

1. **Given** the TUI is running, **When** user presses `b`, **Then** a background-process list (RAM/disk writers) is shown.
2. **Given** the TUI is running, **When** user presses `i`, **Then** the idle-offender table (PID, APP, IDLE, WRITE/h, RAM, REASON) is shown.
3. **Given** a process is selected in the idle view, **When** user presses `k`, **Then** a confirmation modal appears; the process is only closed/killed after explicit confirmation and respects the system-critical blocklist.

---

### User Story 6 — Cleaner Rule Packs (TOML, No Code) (Priority: P6)

As a user with a niche app whose cache isn't built in, I want to add a `[[category]]` entry to a TOML rule pack (id, roots, risk) so sweep cleans it without waiting for a code release.

**Why this priority**: Lowers the barrier to supporting new apps and demonstrates the safety model extends to user-supplied rules.

**Independent Test**: Can be tested by adding a custom `[[category]]` to a rule pack file, running `sweep clean --scan-only`, and verifying the custom category appears with its computed size.

**Acceptance Scenarios**:

1. **Given** a rule pack TOML with `[[category]] id="myapp-cache" roots=["%LOCALAPPDATA%/MyApp/Cache"] risk="Safe"`, **When** user runs `sweep clean --scan-only`, **Then** the category appears with a computed size and `Safe` risk.
2. **Given** a custom category declares `risk="System"`, **When** it is discovered, **Then** it is hidden unless `--deep` is passed (same policy as built-in system categories).
3. **Given** a rule pack path is invalid, **When** sweep loads it, **Then** it logs a clear error and falls back to built-in categories rather than aborting.

---

### User Story 7 — Distribution Polish (Priority: P7)

As a user who downloads the binary, I want a smaller release artifact (release profile tuned), a `sweep --version` that checks for updates, and first-class winget/scoop manifests, so installing and staying current is effortless.

**Why this priority**: Distribution lowers friction for adoption but is not safety- or trust-critical; it is the final polish layer.

**Independent Test**: Can be tested by building with the tuned `[profile.release]`, measuring binary size, and running `sweep --version` to confirm it reports and (when online) checks for a newer release.

**Acceptance Scenarios**:

1. **Given** the project is built with `[profile.release]` (`strip`, `lto`, `codegen-units`), **When** the release binary is produced, **Then** its size is meaningfully smaller than the untuned ~7.8 MB baseline.
2. **Given** `sweep --version` runs, **When** a newer release exists (online), **Then** it reports the current version and a hint that an update is available.
3. **Given** a release is tagged, **When** the release workflow runs, **Then** `sweep-linux-x64` is attached as an artifact and winget/scoop manifests are generated/updated.

---

### Edge Cases

- What happens when `sweep.toml` has a malformed exclusion? — Sweep logs the parse error and proceeds with no exclusions for the bad file; it does not crash.
- What happens when `sweep undo` is run with no journal? — Sweep reports "no session to undo" and exits cleanly.
- What happens when the Recycle Bin is purged before `undo`? — Documented limitation: purged items are gone; `undo` reports unrecoverable.
- What happens when `--kill` targets a process that already exited? — Sweep reports it as already gone and skips (no error).
- What happens when `--allow-kill` guard encounters a system-critical PID writing heavily? — Blocklist always wins; guard never touches it and logs the skip.
- What happens when a rule-pack category root doesn't exist? — Category is skipped silently, same as built-in discovery of missing roots.
- What happens when `sweep doctor` runs without elevation? — It reports `elevation: not` and notes which actions (standby purge, service stop) require elevation, but still completes.
- What happens when `--version` check is offline? — Reports current version and notes the check was skipped (no error).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `sweep doctor` MUST report reserve status (`ok` / `missing` / `consumed`), elevation state, and toast availability.
- **FR-002**: `sweep doctor` MUST report guard armed state (`armed` / `not armed`) and the total size + category list guard would clean if triggered.
- **FR-003**: `sweep doctor` MUST report the current count of detected idle offenders (e.g., `idle: N offenders`).
- **FR-004**: Exclusions MUST be loadable from a `sweep.toml` file supporting path and category-id excludes (and globs).
- **FR-005**: Every cleaner and guard path (diagnose, clean, guard) MUST honor `sweep.toml` exclusions; excluded items MUST be pruned before size calculation and logged as excluded.
- **FR-006**: `sweep clean` MUST record a per-session journal of items moved to the Recycle Bin.
- **FR-007**: `sweep undo` MUST restore the most recent session's trashed items to their original paths.
- **FR-008**: `sweep undo` MUST detect when the Recycle Bin has been purged and report items as unrecoverable rather than failing silently.
- **FR-009**: `sweep idle --close` MUST send a graceful close (WM_CLOSE / non-forced taskkill) to selected idle offenders.
- **FR-010**: `sweep idle --kill --force` and `sweep bg --kill` MUST require an explicit per-process confirmation prompt (`confirm("kill <name> PID <pid> <size>?")`) before terminating.
- **FR-011**: Termination MUST always exclude system-critical PIDs/names via a hard blocklist (PID 0/4, `csrss`, `wininit`, `services`).
- **FR-012**: Guard with `--allow-kill` MUST only perform graceful close (tier 2) on idle offenders writing `>500 MB/h` for `>60m`, never forced kill.
- **FR-013**: Guard without `--allow-kill` MUST never close or kill any process (trim-only), per Principle II.
- **FR-014**: The TUI MUST provide a background-process view (`b`), an idle-offender view (`i`), and a kill action (`k`) with a confirmation modal that respects the blocklist.
- **FR-015**: Cleaner rule packs MUST be loadable from TOML declaring `[[category]]` with `id`, `roots`, and `risk` (Safe/System).
- **FR-016**: Custom categories MUST follow the same risk/visibility policy as built-ins (System hidden unless `--deep`).
- **FR-017**: Invalid rule-pack or `sweep.toml` input MUST be logged with a clear error and fall back to built-in behavior rather than aborting.
- **FR-018**: The release profile MUST enable `strip`, `lto`, and `codegen-units` to reduce binary size.
- **FR-019**: `sweep --version` MUST report the current version and, when online, check for a newer release and hint if available.
- **FR-020**: Releases MUST attach `sweep-linux-x64` and generate/update winget + scoop manifests.
- **FR-021**: All new public items (modules, types, functions) MUST carry doc comments per Constitution Principle VII.

### Key Entities

- **DoctorReport**: Pre-flight snapshot — reserve status, elevation, toast availability, guard armed state, would-clean size/categories, idle offender count.
- **ExclusionConfig**: Parsed `sweep.toml` — list of excluded paths, category ids, and globs.
- **UndoJournal**: Per-session record of trashed items (original path, trash entry) enabling `sweep undo`.
- **KillRequest**: A requested termination — target PID/name, mode (close/kill), and consent flag; validated against the system-critical blocklist.
- **TuiView**: TUI navigation state — `b` (background), `i` (idle), `k` (kill modal).
- **RulePackCategory**: User-supplied `[[category]]` — id, roots, risk, integrated into discovery.
- **ReleaseProfile / Manifest**: Build-and-distribute metadata — tuned profile, version-check source, winget/scoop manifest outputs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `sweep doctor` on a healthy system reports `reserve: ok`, `guard: armed`, elevation + toast state, and a non-empty would-clean size within 5 seconds.
- **SC-002**: An excluded path/category in `sweep.toml` is absent from `sweep diagnose`, `sweep clean --scan-only`, and `sweep guard` (dry) output for that run.
- **SC-003**: `sweep clean -y` followed by `sweep undo` restores the cleaned items to their original locations (when Recycle Bin not purged).
- **SC-004**: `sweep idle --close` exits a known idle test app gracefully; `sweep idle --kill --force` shows a confirmation prompt and never targets a blocklisted PID.
- **SC-005**: Guard with `--allow-kill` performs only graceful close on an idle offender exceeding `>500 MB/h` for `>60m`, and never forced-kills it.
- **SC-006**: The TUI switches between background (`b`) and idle (`i`) views and shows a kill confirmation modal (`k`) that respects the blocklist.
- **SC-007**: A custom `[[category]]` from a rule pack appears in `sweep clean --scan-only` with a computed size and correct risk visibility.
- **SC-008**: The tuned release binary is smaller than the ~7.8 MB untuned baseline, and `sweep --version` reports current version (+ update hint when online).
- **SC-009**: A tagged release attaches `sweep-linux-x64` and produces winget/scoop manifests.
- **SC-010**: CI (`ubuntu-latest` + `windows-latest`) is green for the Stage 3 changes; `cargo test --locked` passes on both OS.

## Assumptions

- Target platforms: Windows 10/11 (primary) and Linux (parity). `WM_CLOSE` is Windows-specific; Linux graceful close uses `SIGTERM` equivalent via the existing process abstraction.
- `sweep.toml` lives in the project or user config dir; if absent, no exclusions apply (backward compatible).
- Undo relies on the OS Recycle Bin / trash retaining items; once purged, recovery is impossible (documented limitation, per Principle II).
- Rule-pack loading reuses the existing `CleanCategory` discovery flow; user-supplied rules are additive, not a replacement for built-ins.
- The `--version` check reads a lightweight release-metadata endpoint; offline is a non-error degraded mode.
- Release profile tuning does not change runtime behavior, only binary size/speed; CI must still build on both OS.
- Controlled termination continues to honor Constitution Principle II graduated escalation (trim → close → kill) and the audit log in `%LOCALAPPDATA%\sweep\guard.log`.
- EUD (elevation) is required for standby purge and service stop; doctor and kill paths degrade gracefully without it.
- Backlog items (watch-folder alerts, `sweep self-uninstall`, large/old-file wizard) remain out of scope for Stage 3.
