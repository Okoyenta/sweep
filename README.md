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
| `sweep clean [--scan-only] [--only ids...] [-y]` | scan and reclaim cache/temp categories (Recycle Bin / trash backed) |
| `sweep ram [--trim-top N] [--purge-standby]` | trim process working sets; purge standby list / kernel caches |
| `sweep tui [--top N]` | live dashboard (auto-refresh 2 s; `q` quit, `r` refresh, `t` trim top-10, `p` purge standby) |
| `sweep bin [--empty] [-y]` | list recycle bin contents; permanently empty it |
| `sweep dupes [--min-mb N] [--trash-group N] [-y]` | duplicate-file groups from the index, sorted by wasted bytes |
| `sweep schedule --install\|--remove\|--status` | daily background re-index (schtasks / crontab) |

## Cleanable categories (v1)

- **Windows**: user temp files, crash dumps, Chrome/Edge caches (cache,
  code cache, GPU cache), npm cache, pip cache, per-profile Firefox cache2.
- **Linux**: browser caches under `$XDG_CACHE_HOME` (Chrome, Chromium, Edge,
  Brave), Firefox profile cache2, thumbnails, pip, fontconfig, `~/.npm/_cacache`.

Everything is moved to the **Recycle Bin / trash** first — nothing is deleted
permanently by `clean`. Note the Recycle Bin still occupies disk until emptied.

## Architecture

Strict layering, enforced by convention:

```
ui ──▶ services ──▶ domain          domain has zero OS imports;
         ▲              ▲           infra implements domain traits
         └── infra ─────┘
```

- `src/domain/` — models (`SystemSnapshot`, `EntryRecord`, `AppUsage`,
  `InstalledApp`, `CleanCategory`, ...) and ports (`SystemMonitor`,
  `UsageProbe`, `IndexStore`, `AppInventory`, `PathRemover`, `RamTrimmer`).
- `src/services/` — orchestration & merge policies (`IndexService`,
  `UsageService`, `AppService`, `CleanService`, `RamService`, `SystemService`),
  all unit-tested against mocks.
- `src/infra/` — OS bindings:
  - `sysinfo_monitor`, `sqlite_store` (WAL), `walker` (jwalk-based,
    incremental with DB-seeded skip queue), `trash_remover`.
  - `win/`: Prefetch + UserAssist (ROT13) usage probes, registry uninstall-key
    inventory (64/32-bit views), cleaner path discovery, `EmptyWorkingSet`
    trimmer + `NtSetSystemInformation` standby purge.
  - `linux/`: desktop-file app inventory, XDG cache discovery,
    `/proc/sys/vm/drop_caches` purge.
- `src/ui/` — clap CLI definitions, plain-text printers, ratatui dashboard.

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

## Status / roadmap

1. ✅ skeleton, system status
2. ✅ SQLite index + incremental walker (resumable)
3. ✅ usage probes (Prefetch + UserAssist)
4. ✅ app inventory + official uninstaller launch (Windows)
5. ✅ cleaners (Windows-first, trash-backed)
6. ✅ RAM tools (working-set trim, standby/drop_caches purge)
7. ✅ TUI dashboard
8. ✅ Linux modules + README (desktop inventory, XDG cleaners, drop_caches)

Possible next steps: content-aware duplicate merging rules (keep-newest),
scheduled task with missed-run catch-up, TUI-driven clean/bin flows.
