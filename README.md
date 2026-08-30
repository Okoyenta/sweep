# sweep

A lightweight, cross-platform (Windows + Linux) TUI/CLI utility that monitors
RAM and disk usage, indexes files and installed apps in the background,
detects unused items, and helps you free space and memory on demand.

Design goals: **< 50 MB RAM**, **~0 idle CPU**, strict layered architecture.

Upcoming work and design plans: see [ROADMAP.md](ROADMAP.md).

## Install

Download the binary for your OS from the
[latest release](https://github.com/Okoyenta/sweep/releases/latest) and put it
on your `PATH`:

```console
# Windows
curl -L -o sweep.exe https://github.com/Okoyenta/sweep/releases/latest/download/sweep-windows-x64.exe

# Linux
curl -L -o sweep https://github.com/Okoyenta/sweep/releases/latest/download/sweep-linux-x64
chmod +x sweep
```

Once the package is published, winget is the intended install path and puts
`sweep` on your `PATH` automatically (open a new terminal afterwards):

```console
winget install Okoyenta.Sweep
```

Package-manager install is wired up in the release workflow but needs one-time
account setup before it works — see
[docs/DISTRIBUTION.md](docs/DISTRIBUTION.md). There is no MSI/Inno installer
(winget handles `PATH`, so one isn't needed for a CLI tool) and the binary is
unsigned.

## Commands

| Command | Description |
| --- | --- |
| `sweep` | same as `sweep status --top 10` |
| `sweep status [--top N]` | memory/swap bars, disk usage, top RAM processes with last-run times, index stats |
| `sweep index [--status] [--full] [roots...]` | build/resume the background file index (incremental by default) |
| `sweep apps [--since-days N] [--uninstall NAME]` | list installed apps (name/version/size/last-run); launch official uninstallers on Windows |
| `sweep clean [--scan-only] [--only ids...] [-y] [--deep] [--stop-services] [--kill]` | scan and reclaim cache/temp categories (Recycle Bin / trash backed); `--deep` includes system categories (WU/DO/WinSxS/driver store), `--stop-services` stops wuauserv+bits via RAII, `--kill` detects lock-holding processes (handle-table scan on Windows, name-heuristic on Linux), shows popup and kills on confirm (`-y` skips popup) |
| `sweep ram [--trim-top N] [--purge-standby]` | trim process working sets; purge standby list / kernel caches |
| `sweep tui [--top N]` | live dashboard (auto-refresh 2 s; `q` quit, `r` refresh, `t` trim top-10, `p` purge standby, `b` background view, `i` idle-writer view, `↑`/`↓` select, `k` kill with confirmation) |
| `sweep bin [--empty] [-y]` | list recycle bin contents; permanently empty it |
| `sweep dupes [--min-mb N] [--trash-group N] [-y]` | duplicate-file groups from the index, sorted by wasted bytes |
| `sweep diagnose [--deep]` | scan safe + system categories with per-category hints; `--deep` includes WU downloads, DO cache, WinSxS, driver store (risk=System) |
| `sweep schedule --install\|--remove\|--status` | daily background re-index (schtasks / crontab) |
| `sweep schedule --guard-install\|--guard-remove\|--guard-status` | register guard daemon on logon via scheduled task |
| `sweep guard [--once] [--ram-threshold N] [--disk-min-gb N] [--interval-secs N] [--allow-service-stop] [--allow-kill]` | background daemon: trims RAM on pressure, graduated disk rescue; `--once` single-pass |
| `sweep idle [--top N] [--idle-mins N] [--min-write-mb N] [--clean-cache]` | detect idle heavy SSD writers via two-snapshot I/O diff; `--clean-cache` cleans whitelisted dirs |
| `sweep idle --close [--only PID...]` | graceful close (`WM_CLOSE` / `SIGTERM`) of idle offenders |
| `sweep idle --kill --force [--only PID...]` | forced kill, after a per-process `kill <name> PID <pid> <size>?` confirmation |
| `sweep bg [--top N] [--kill --force] [--only PID...]` | background-process list; same consent + blocklist rules as `idle` |
| `sweep doctor` | pre-flight safety report: reserve, elevation, toast, guard armed state, idle offenders, per-volume media type, would-clean estimate |
| `sweep optimize` | list volumes with detected media (ssd/hdd) and the maintenance action each implies |
| `sweep optimize --volume C: [--analyze] [-y]` | TRIM a solid-state volume or defragment a rotational one; `--analyze` previews without modifying |
| `sweep undo` | restore the most recent session of trashed items from the Recycle Bin |
| `sweep --version` | print the version and, when online, whether a newer release is available |

Two global flags apply to every command: `--config <path>` selects a specific
`sweep.toml`, and `--rules <path>` loads an extra cleaner rule pack.

## Trust & control

### Exclusions and custom cleaners (`sweep.toml`)

`sweep.toml` is looked up in this order: `--config <path>`, then `./sweep.toml`,
then the user config dir (`%LOCALAPPDATA%\sweep\sweep.toml` on Windows,
`~/.config/sweep/sweep.toml` on Linux). If no file is found, sweep behaves
exactly as it does without one. A malformed file is reported and then ignored —
it never aborts a run.

```toml
[exclusions]
paths = ["C:/Games/Cache"]        # this tree is never scanned or cleaned
category_ids = ["dev-pnpm"]       # skip a category entirely
globs = ["**/node_modules/**"]    # matched case-insensitively

[[category]]                      # add a cleaner without a code change
id = "myapp-cache"
roots = ["%LOCALAPPDATA%/MyApp/Cache"]
risk = "Safe"                     # or "System" (hidden unless --deep)
# cleanup_command = "myapp --clear"
```

Exclusions are honored by `diagnose`, `clean`, and `guard`, and excluded paths
are pruned *before* their size is measured, so excluded space is never counted
or touched. When anything is excluded, sweep prints an `excluded: N` line.

### Undo

Every `sweep clean` (and every guard disk rescue) records what it moved to the
Recycle Bin. `sweep undo` restores the most recent session:

```console
sweep clean -y --only user-temp
sweep undo
```

**Limitation:** undo relies on the OS Recycle Bin still holding the items. If the
Bin has been emptied — manually, by `sweep bin --empty`, or by guard's
documented auto-purge fallback — those items are gone for good. `sweep undo`
reports each such item as `unrecoverable (recycle bin purged)` and exits 0
rather than failing silently. Only the newest session is restorable; the journal
keeps the last 20 for audit.

### Process termination

Termination is graduated and never silent (Constitution Principle II):

1. **Trim** — `sweep ram --trim-top N`, always safe, the default guard action.
2. **Graceful close** — `sweep idle --close` sends `WM_CLOSE` / `SIGTERM`.
3. **Forced kill** — `sweep idle --kill --force` or `sweep bg --kill --force`
   prompts `kill <name> PID <pid> <size>?` for each process and acts only on an
   explicit yes.

A hard blocklist (PID 0, PID 4, `csrss`, `wininit`, `services`, and sweep's own
PID) is always applied first and cannot be overridden by any flag. `sweep guard`
never kills: without `--allow-kill` it is trim-only, and with `--allow-kill` it
performs a graceful close only, and only for processes writing more than
500 MB/h while idle for over 60 minutes. Every decision is written to
`%LOCALAPPDATA%\sweep\guard.log`.

### Drive maintenance (`sweep optimize`)

Sweep frees space; `optimize` keeps the drive healthy afterwards. It picks the
action from the drive's physical media, and **never guesses**:

| Media | Action | Why |
| --- | --- | --- |
| Solid-state | TRIM (`Optimize-Volume -ReTrim` / `fstrim`) | Tells the drive which blocks are free after a clean. Defragmenting flash would burn write cycles for no seek benefit. |
| Rotational | Defragment (`Optimize-Volume -Defrag`) | Clearing scattered caches leaves free-space fragmentation that costs seeks. Not available on Linux — ext4/xfs/btrfs manage extents themselves. |
| Unknown | Nothing | Sweep refuses rather than risk defragmenting an SSD. |

```console
sweep optimize                          # list volumes and their media
sweep optimize --volume C: --analyze    # preview, modifies nothing
sweep optimize --volume C:              # confirms, then runs
```

Media type comes from the device itself — the seek-penalty storage ioctl on
Windows, `/sys/block/<dev>/queue/rotational` on Linux — so detection is instant
and needs no elevation. Running the maintenance **does** require elevation
(Administrator / root); sweep says so before prompting rather than failing
afterwards. Defrag is long-running and I/O-heavy, so it always confirms unless
`-y` is passed.

## Cleanable categories (v1)

- **Windows**: user temp files, crash dumps, Chrome/Edge caches (cache,
  code cache, GPU cache), npm cache, pip cache, **pnpm store** (via
  `pnpm store prune`, not trash — hardlinks require the tool's own prune),
  per-profile Firefox cache2. With `--deep`: Windows Update downloads,
  Delivery Optimization cache, WinSxS reclaimable bytes (via DISM), driver
  store (risk=System).
- **Linux**: browser caches under `$XDG_CACHE_HOME` (Chrome, Chromium, Edge,
  Brave), Firefox profile cache2, thumbnails, pip, fontconfig, `~/.npm/_cacache`.

Everything is moved to the **Recycle Bin / trash** first — nothing is deleted
permanently by `clean` (except `pnpm store` which delegates to `pnpm store prune`
for correctness). Note the Recycle Bin still occupies disk until emptied
(`sweep bin --empty`). When `clean` reports `failed (locked or protected)`,
re-run with `--kill` to detect and optionally kill the holding processes.

`diagnose` threads a per-category `hint` (e.g. `pnpm hardlinks`, `DISM /Cleanup-Image`,
`Managed by sweep`) and `diagnose --deep` rolls up `safe_reclaimable` vs
`system_reclaimable`.

## Architecture

Strict layering, enforced by convention:

```
ui ──▶ services ──▶ domain          domain has zero OS imports;
         ▲              ▲           infra implements domain traits
         └── infra ─────┘
```

- `src/domain/` — models (`SystemSnapshot`, `EntryRecord`, `AppUsage`,
  `InstalledApp`, `CleanCategory` (+ `risk`, `cleanup_command`), `CategoryScan`,
  `CleanOutcome` (+ `failed_paths`), `LockedProcess`, `GuardConfig`, `RamSnapshot`,
  `DiskSnapshot`, `IdleSsdOffender`, `DeepScanResult`, `DiagnoseRow` (+ `hint`),
  ...) and ports (`SystemMonitor`, `UsageProbe`, `IndexStore`, `AppInventory`,
  `PathRemover`, `RamTrimmer`, `GuardMonitor`).
- `src/services/` — orchestration & merge policies (`IndexService`,
  `UsageService`, `AppService`, `CleanService` (trash + `cleanup_command` +
  `failed_paths`), `RamService`, `SystemService`, `GuardService`, `IdleService`,
  `BenchmarkRecorder`), all unit-tested against mocks.
- `src/infra/` — OS bindings:
  - `sysinfo_monitor`, `sqlite_store` (WAL), `walker` (jwalk-based,
    incremental with DB-seeded skip queue), `trash_remover`.
  - `win/`: Prefetch + UserAssist (ROT13) usage probes, registry uninstall-key
    inventory (64/32-bit views), cleaner path discovery, `EmptyWorkingSet`
    trimmer + `NtSetSystemInformation` standby purge, `deep_clean` (WU/DO/WinSxS/
    driver store scanner), `service_lock` (RAII service stop/start for
    wuauserv+bits), `process_lock` (handle-table scan via
    `NtQuerySystemInformation(64)` + `DuplicateHandle` + `GetFinalPathNameByHandleW`
    with runtime File-type-index discovery and `GetFileType` filter; cached
    `OpenProcess` per PID, 15 s deadline, early exit).
  - `linux/`: desktop-file app inventory, XDG cache discovery,
    `/proc/sys/vm/drop_caches` purge, `process_lock` (name-heuristic per
    category via `sysinfo`).
- `src/ui/` — clap CLI definitions, plain-text printers (`clean` prints
  `print_kill_list`, `diagnose` threads `hint`), `guard` popups (`confirm_kill`
  via PowerShell MsgBox with dedup, `-y` skips), ratatui dashboard.

The file index lives in `%LOCALAPPDATA%\sweep\index.db`
(`~/.local/share/sweep/index.db` conceptually on Linux), crash-safe via WAL;
interrupted runs resume.

## Building

Requirements: Rust 1.98+ (stable).

Windows (GNU toolchain + WinLibs MinGW):

```powershell
$env:Path = "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:USERPROFILE\.cargo\bin;" + $env:Path
cargo build --release -j 2   # -j 2 avoids antivirus/linker races on this setup
```

Linux: `cargo build --release` (bundled SQLite needs a C compiler, e.g. `gcc`).

Cross-checking the Linux code paths from Windows is limited: bundled SQLite
requires an `x86_64-linux-gnu-gcc` cross toolchain. The crate's own Linux
modules are pure std and mirror the Windows logic.

## Platform notes

- **Prefetch** is empty/disabled on some Windows installs; last-run data then
  comes from UserAssist only (Explorer-launched programs). UWP/AUMID-only
  launches (e.g. Start-menu "Chrome") are not attributed to exes.
- **Standby-list purge** requires an elevated shell (SeProfileSingleProcessPrivilege).
- **Linux working-set trim** has no portable equivalent; use `--purge-standby`
  as root instead.
- **Uninstallers** on Windows run through the vendor's own uninstall string
  (UAC prompt appears if required). On Linux, sweep points you at your package
  manager rather than guessing commands.
- **`--kill`** is opt-in and off by default; it never kills `sweep` itself and
  dedupes by `(pid, path)`. On Windows it uses a handle-table scan (needs
  `SeDebugPrivilege` for some system processes; falls back to leaving files);
  on Linux it uses a `sysinfo` name-heuristic per category. With `-y` the popup
  is skipped (kills immediately after printing the list).
- **`pnpm store`** uses hardlinks — sweep delegates to `pnpm store prune` instead
  of trashing.
- **`--deep` + driver-store** reports `System` risk; reclaim requires
  `Dism /Online /Cleanup-Image /StartComponentCleanup` (elevated) or
  `sweep clean --deep --stop-services` for WU/DO unlock.

## Status / roadmap

1. ✅ skeleton, system status
2. ✅ SQLite index + incremental walker (resumable)
3. ✅ usage probes (Prefetch + UserAssist)
4. ✅ app inventory + official uninstaller launch (Windows)
5. ✅ cleaners (Windows-first, trash-backed; pnpm via `pnpm store prune`, hints in diagnose)
6. ✅ RAM tools (working-set trim, standby/drop_caches purge)
7. ✅ TUI dashboard
8. ✅ Linux modules + README (desktop inventory, XDG cleaners, drop_caches)
9. ✅ guard daemon (RAM pressure trim, disk rescue, cooldown, single-instance lock, toast, logging)
10. ✅ deep system scan (WU downloads, DO cache, WinSxS, driver store — risk=System)
11. ✅ service-aware unlock (RAII stop/start for wuauserv+bits via `--stop-services`)
12. ✅ idle SSD offender detection (two-snapshot I/O diff, `--clean-cache`)
13. ✅ full benchmark visibility (per-category breakdown, safe/system split in diagnose)
14. ✅ guard autostart (`sweep schedule --guard-install` / `--guard-remove` / `--guard-status`)
15. ✅ `clean --kill` (handle-table scan with `GetFileType` filter, runtime File-type-index discovery, `-y` skips popup; Linux name-heuristic)
16. ✅ pnpm hardlink-aware clean + diagnose hints + `clean --only` scan optimization (avoids walking pnpm store when filtered)

Possible next steps: content-aware duplicate merging rules (keep-newest),
scheduled task with missed-run catch-up, TUI-driven clean/bin flows.
