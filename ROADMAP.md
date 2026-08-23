# sweep — roadmap

Status: milestones 1–8 complete and shipped as v0.8.0 (status, index, usage
probes, app inventory, cleaners, RAM tools, TUI dashboard, Linux modules +
recycle bin, dupes, schedule, TUI actions). Also done since: `sweep du`
space insights, dupes v2 (DB hash cache), TUI control-center actions
(clean / dupes / bin / schedule). See `README.md` for what exists today.

## Next up: `sweep guard` (auto-rescue daemon) — design locked

A resident background mode that intervenes before the machine hangs.

**Confirmed decisions**

- Disk rescue: **auto-clean + Windows toast notification** (not silent,
  not notify-only). Safe categories include npm/pip caches.
- If free space is still below minimum **after** the cache rescue:
  **auto-purge the Recycle Bin** (with its own toast).
- Defaults locked: RAM ≥ 90 %, disk < 2 GB free, poll every 30 s.
- License: **MIT** (`LICENSE` file + `license = "MIT"` in Cargo.toml).

**Core loop** (`sweep guard [--ram-threshold 90] [--disk-min-gb 2] [--interval-secs 30] [--once]`)

- Poll every N seconds (near-zero CPU while healthy); single-instance lock
- **RAM pressure**: used ≥ threshold for 3 consecutive samples → trim top-10
  working sets (+ standby purge when elevated); log + toast
- **Disk pressure**: free < min → trash-backed clean of safe categories
  (user temp, browser caches, npm/pip) → still low → purge Recycle Bin
- Rolling log at `%LOCALAPPDATA%\sweep\guard.log`; toast via PowerShell
  WinRT one-liner with graceful no-op if unavailable
- Cooldown (~10 min) between rescues so warnings/actions never spam

**Autostart**: extend `sweep schedule` with a logon-task variant
(`schtasks /SC ONLOGON`) pointing at `sweep guard`.

**Safety rails**: whitelisted categories only, everything trash-backed,
never kills processes (working-set trim only), cooldown enforced.

## Phase 2 (after guard): adoption features

Goal: make sweep trustworthy enough that people let it run unattended.

| # | Feature | Why |
|---|---------|-----|
| 1 | **`sweep doctor`** | Pre-flight check: what guard would do right now, elevation state, toast support, category sizes — builds trust before enabling autostart |
| 2 | **Exclusions via `sweep.toml`** | Per-user ignore lists (paths, apps, categories) honored by every cleaner + guard; required for "leave my stuff alone" |
| 3 | **Undo journal** | Record trashed items per session → `sweep undo`. Limitation to document: once the Recycle Bin is purged (manually or by guard), undo is impossible |
| 4 | **Cleaner rule packs** | Data-driven TOML rules for new apps without code changes |

## Phase 3: distribution & credibility

- Release-profile tuning: `Cargo.toml` has no `[profile.release]` section yet
  (`strip`, `lto`, `codegen-units`) — quick win to shrink the ~7.8 MB binary
- GitHub Actions matrix builds (Windows + Linux) — also solves the Linux
  cross-compile problem below
- winget + scoop manifests; `sweep --version` update check
- Medium-value ideas (backlog): watch-folder alerts, `sweep self-uninstall`,
  first-run doctor on launch, large/old-file cleanup wizard in TUI

## Known limitations to revisit

- Cross-building from Windows can't compile bundled SQLite for Linux
  (needs `x86_64-linux-gnu-gcc`); verify Linux builds on CI or a real box
- Prefetch empty/disabled on some systems → last-run data limited to
  UserAssist (Explorer-launched programs); UWP/AUMID launches unattributed
- Standby purge requires elevation (`SeProfileSingleProcessPrivilege`)
- Recycle Bin occupies disk until emptied — guard's auto-purge accounts
  for this, but freed-space reporting should stay honest about it
