# sweep

A lightweight, cross-platform (Windows + Linux) TUI/CLI utility that monitors
RAM and disk usage, indexes files and installed apps in the background,
detects unused items, and helps you free space and memory on demand.

Design goals: **< 50 MB RAM**, **~0 idle CPU**, strict layered architecture.

Upcoming work and design plans: see [ROADMAP.md](ROADMAP.md).

## Commands

| Command | Description |
| --- | --- |
| `sweep` | same as `sweep status --top 10` |
| `sweep status [--top N]` | memory/swap bars, disk usage, top RAM processes with last-run times, index stats |
| `sweep index [--status] [--full] [roots...]` | build/resume the background file index (incremental by default) |
| `sweep apps [--since-days N] [--uninstall NAME]` | list installed apps (name/version/size/last-run); launch official uninstallers on Windows |
| `sweep clean [--scan-only] [--only ids...] [-y] [--deep] [--stop-services] [--kill]` | scan and reclaim cache/temp categories (Recycle Bin / trash backed); `--deep` includes system categories (WU/DO/WinSxS/driver store), `--stop-services` stops wuauserv+bits via RAII, `--kill` detects lock-holding processes (handle-table scan on Windows, name-heuristic on Linux), shows popup and kills on confirm (`-y` skips popup) |
| `sweep ram [--trim-top N] [--purge-standby]` | trim process working sets; purge standby list / kernel caches |
| `sweep tui [--top N]` | live dashboard (auto-refresh 2 s; `q` quit, `r` refresh, `t` trim top-10, `p` purge standby) |
| `sweep bin [--empty] [-y]` | list recycle bin contents; permanently empty it |
| `sweep dupes [--min-mb N] [--trash-group N] [-y]` | duplicate-file groups from the index, sorted by wasted bytes |
| `sweep diagnose [--deep]` | scan safe + system categories with per-category hints; `--deep` includes WU downloads, DO cache, WinSxS, driver store (risk=System) |
| `sweep schedule --install\|--remove\|--status` | daily background re-index (schtasks / crontab) |
| `sweep schedule --guard-install\|--guard-remove\|--guard-status` | register guard daemon on logon via scheduled task |
| `sweep guard [--once] [--ram-threshold N] [--disk-min-gb N] [--interval-secs N] [--allow-service-stop] [--allow-kill]` | background daemon: trims RAM on pressure, graduated disk rescue; `--once` single-pass |
| `sweep idle [--top N] [--idle-mins N] [--min-write-mb N] [--clean-cache]` | detect idle heavy SSD writers via two-snapshot I/O diff; `--clean-cache` cleans whitelisted dirs |

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
