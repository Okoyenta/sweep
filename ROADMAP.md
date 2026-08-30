# sweep — roadmap

Status: milestones 1–16 complete and shipped (status, index, usage probes,
app inventory, cleaners incl. pnpm hardlink-aware prune, RAM tools, TUI,
Linux modules, recycle bin, dupes, schedule, guard daemon, deep scan
(WU/DO/WinSxS/driver store), service-aware unlock (`--stop-services`), idle
offender detection, benchmark visibility, guard autostart, `clean --kill`
(handle-table scan + popup, `-y` skips), diagnose hints, `clean --only`
scan optimization). See `README.md` for what exists today.

## Shipped: `sweep guard` (auto-rescue daemon)

**Decisions (implemented)**

- Disk rescue: **auto-clean + Windows toast notification** (not silent,
  not notify-only). Safe categories include npm/pip/pnpm caches.
- If free space is still below minimum **after** the cache rescue:
  **auto-purge the Recycle Bin** (with its own toast).
- Defaults: RAM ≥ 90 %, disk < 2 GB free, poll every 30 s.
- License: **MIT** (`LICENSE` file + `license = "MIT"` in Cargo.toml).

**Core loop** (`sweep guard [--ram-threshold 90] [--disk-min-gb 2] [--interval-secs 30] [--once] [--allow-service-stop] [--allow-kill]`)

- Poll every N seconds (near-zero CPU while healthy); single-instance lock (`guard.lock`)
- **RAM pressure**: used ≥ threshold for 3 consecutive samples → trim top-10
  working sets (+ standby purge when elevated); log + toast
- **Disk pressure**: free < min → trash-backed clean of safe categories
  (user temp, browser caches, npm/pip/pnpm) → still low → purge Recycle Bin
- Deep/system categories are opt-in via `--deep` + `--allow-service-stop` / `--allow-kill`
- Rolling log at `%LOCALAPPDATA%\sweep\guard.log`; toast via PowerShell
  WinRT one-liner with graceful no-op if unavailable
- Cooldown (~10 min) between rescues so warnings/actions never spam

**Autostart**: `sweep schedule --guard-install` / `--guard-remove` / `--guard-status`
(`schtasks /SC ONLOGON` on Windows, crontab on Linux) pointing at `sweep guard`.

**Safety rails**: whitelisted categories only, everything trash-backed by default,
`--allow-kill` / `--allow-service-stop` are opt-in, cooldown enforced.

## Shipped: Stage 3 — Trust & Control (spec `003-trust-control`)

Goal: make sweep trustworthy enough that people let it run unattended.

| # | Feature | Status |
|---|---------|--------|
| 1 | **`sweep doctor`** | Shipped — reserve, elevation, toast, guard armed state, idle offender count, would-clean estimate (time-budgeted so the report stays under 5 s) |
| 2 | **Exclusions via `sweep.toml`** | Shipped — paths / category ids / globs, honored by `diagnose`, `clean`, and `guard`; pruned before sizing |
| 3 | **Undo journal** | Shipped — `sweep clean` and guard rescues journal every trashed item; `sweep undo` restores the newest session and reports purged items as unrecoverable |
| 4 | **Controlled termination** | Shipped — `idle --close` (graceful), `idle --kill --force` / `bg --kill --force` behind a per-process confirm, hard system blocklist, guard `--allow-kill` is graceful-close-only |
| 5 | **TUI `b` / `i` / `k`** | Shipped — background and idle views, row selection, kill confirmation modal that refuses blocklisted processes |
| 6 | **Cleaner rule packs** | Shipped — TOML `[[category]]` with env-var expansion, id-collision and missing-root handling, `System` risk hidden unless `--deep`, `--rules <path>` for extra packs |
| 7 | **Distribution** | Shipped — tuned `[profile.release]`, `sweep --version` update check, Windows + Linux release artifacts, winget + scoop manifests |

## Shipped: drive maintenance (`sweep optimize`)

Closes the "sweep frees space but doesn't maintain the drive" gap.

- Media detection per volume: seek-penalty storage ioctl on Windows,
  `/sys/block/<dev>/queue/rotational` on Linux — instant, no elevation
- SSD → TRIM, HDD → defrag, unknown → refuse (never defrag flash)
- `--analyze` previews; maintenance confirms unless `-y`; elevation reported
  up front instead of failing mid-run
- Media type surfaced on a `storage:` line in `sweep doctor`

## Phase 4: backlog

- **Reclaim storage from the TUI** — the TUI shows disk gauges but cannot act on
  them: its only actions are `t` (RAM trim), `p` (standby purge), and `k` (kill).
  Freeing disk space means quitting and running `sweep clean`. Proposal: a `c`
  key opening a category list with sizes and a confirm modal, reusing the `k`
  modal pattern. Needs the same treatment as `doctor` — a time budget or a
  background scan with progress — because `clean --scan-only` can take minutes
  and must not freeze the UI on a synchronous walk.
- **A real Windows installer** — *deprioritized: winget is the chosen install
  path.* winget's portable mode already puts `sweep` on the user `PATH` via its
  Links directory, which was the main reason to want an installer. A WiX (MSI)
  or Inno Setup package would only add a Start Menu entry, an Add/Remove
  Programs entry, and Group Policy / Intune deployment. Revisit if enterprise
  deployment is ever needed. Pairs with code signing, without which SmartScreen
  warns on direct download.
- Medium-value ideas: watch-folder alerts, `sweep self-uninstall`,
  first-run doctor on launch, large/old-file cleanup wizard in TUI

## Known limitations to revisit

- Cross-building from Windows can't compile bundled SQLite for Linux
  (needs `x86_64-linux-gnu-gcc`); verify Linux builds on CI or a real box
- Prefetch empty/disabled on some systems → last-run data limited to
  UserAssist (Explorer-launched programs); UWP/AUMID launches unattributed
- Standby purge requires elevation (`SeProfileSingleProcessPrivilege`)
- Recycle Bin occupies disk until emptied — guard's auto-purge accounts
  for this, but freed-space reporting should stay honest about it
