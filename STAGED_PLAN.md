# Sweep — Staged Plan: 3 stages, each fixes multiple fixtures

> Consolidates `ROADMAP.md:1`, `SPACE_RESERVE.md:1`, Windows Storage Optimizer skill (6 features), background/idle SSD hogs, and constitution `v1.1.0` amendment (allow graduated kill). Each stage ships independently and unblocks the next.

**Current base:** milestones 1–8 shipped `v0.8.0` (`README.md:97` + `ROADMAP.md:3`): `status`, `index` (WAL+incremental walker `src/infra/walker.rs:112`), `apps`+`usage` (Prefetch `prefetch.rs:40` + UserAssist ROT13 `userassist.rs:153`), `clean` (10 cats `win/clean_paths.rs:17` / `linux/clean_paths.rs:7` → `trash_remover.rs:22`), `ram` (`EmptyWorkingSet` `win/ram.rs:1` + `drop_caches` `linux/ram.rs:27`), `tui`, `bin`, `dupes` (hash cache), `schedule` (`schtasks`/crontab) + CI green `ubuntu+windows` (`ci.yml:6`).

**Fixtures to solve (5 sources):**
| # | Fixture | Source |
|---|---|---|
| F1 | `C: 0 B` lockout — `sweep status :53` SQLite `1546` fails, `cargo build` fails, `clean` needs `empty` | Live `2026-08-23` `7.97 GiB` scan, `SPACE_RESERVE.md:4` |
| F2 | Background apps consuming after close (tray/daemon/renderer) | User report `chrome 232 MiB` still resident |
| F3 | Idle-but-open apps silently writing SSD (cache/log/swap hours unused) | User report idle hours → SSD wear |
| F4 | Windows deep bloat — WU cache, WinSxS, Delivery Optimization, driver leftovers | Optimizer skill key features `03` |
| F5 | Dev cache bloat — `pnpm`/`cargo`/`gradle` beyond `npm`/`pip` (`win/clean_paths.rs:67`) | Optimizer skill `04`, live `npm-cache 6.64 GiB` |
| F6 | No service-aware unlock of locked files | Optimizer skill `02` |
| F7 | No pre/post benchmark, no diagnostics | Optimizer skill `01`/`05` |

---

## Stage 1 — Immediate Relief & Visibility (1–2 days) — fixes F1+F5+F2+F7 (partial)

**Goal:** never get bricked at `0 B` again; see what's hogging now; reclaim dev bloat instantly. No elevation, no service stops, no kills beyond trim (constitution II-tier 1).

**Delivers:**
- **F1 Space Reserve 512 MB** (`SPACE_RESERVE.md:12`): `src/infra/paths.rs:1` `reserve_path()` / `ensure_reserve()` / `consume_reserve()` / `is_disk_full_error()`, `src/main.rs:53` `open_store_with_reserve()` + `run_status:492` fallback (show RAM/disks/top even if index unavailable), `run_clean:247` `ensure_headroom_or_consume_reserve()` (<256 MB) + fix empty-`only` bug (`src/main.rs:235` `Some(&only)`→`None` when empty, `CleanService::run :54`). Re-create reserve after `clean`/`bin --empty`. `SWEEP_DB` env fallback to `D:`.
- **F5 Dev caches scan:** `src/infra/dev_caches.rs` (new, shared win+linux) discovers `pnpm` (`%LOCALAPPDATA%/pnpm/store`), `cargo` (`~/.cargo/registry/cache` + `git/checkouts`), `gradle` (`~/.gradle/caches`), `uv`/`pipx` — added to `discover_categories()` (`win/clean_paths.rs:17` / `linux/clean_paths.rs:81`), tests like `linux/clean_paths.rs:92`.
- **F2+F3 Detection skeleton:** wire `sysinfo 0.39.6` `Process::disk_usage()` into `SystemSnapshot` (`src/domain/models.rs:22` add `read_bytes/write_bytes/total_written`, `src/infra/sysinfo_monitor.rs:69` call `disk_usage()`), `src/infra/win/idle.rs` `WinIdleProbe` (`GetLastInputInfo` + `GetForegroundWindow` via `windows-sys` `Win32_UI_Input` flag) — Linux stub heuristic. `sweep diagnose` (new `src/ui/diagnose.rs`) prints `Category | Size | Risk | Reclaim` sorted, including new dev cats + `potential reclaim` rollup (Optimizer `01`).
- **F7 Benchmark stub:** `sweep clean`/`bin` now snapshots `SysinfoMonitor` before/after and prints `before X free → after Y free (freed Z) in Ns` (`src/ui/clean.rs:27`).

**Ship check:** `sweep diagnose`, `sweep clean --scan-only` shows `pnpm/cargo/gradle` sizes, `sweep status` works at `0 B` by consuming `reserve.bin`, `cargo test --locked` green both OS.

**Fixes together:** F1 unlocks you to run F5 reclaims; F2/F3 visibility makes next stage's auto-actions trustworthy.

---

## Stage 2 — Safe Automation & Deep Cleaning (3–5 days) — fixes F4+F6+F3+F7 (full) + F2 auto

**Goal:** `sweep guard` actually prevents hangs + deep system bloat, with service-aware unlock but still trim-only by default (constitution II-tier 1). System cats stay scan-only unless `--deep`.

**Delivers:**
- **Guard daemon** (`ROADMAP.md:9` design locked, `constitution:8`): `src/main.rs` `run_guard [--ram-threshold 90] [--disk-min-gb 2] [--interval-secs 30] [--once]` loop 30s, 3-sample RAM hysteresis → `RamService::optimize(top10)+purge_standby`, disk `<2 GB` → `consume_reserve()` → trash safe cats → still low → `TrashBin::purge_all()`. Single mutex, `guard.log` (`%LOCALAPPDATA%/sweep/guard.log`), PowerShell WinRT toast (`ROADMAP.md:30`), cooldown 10 min. Autostart via `sweep schedule` `schtasks /SC ONLOGON` variant (`src/infra/schedule.rs:14`).
- **F4 Deep system cats** (scan-only by default): `src/infra/win/deep_clean.rs` discovers `C:\Windows\SoftwareDistribution\Download` (WU), `DeliveryOptimization`, WinSxS analysis via `dism /Online /Cleanup-Image /AnalyzeComponentStore` (parse, never delete raw), driver store age. Added with `risk=System` (`domain/models.rs:CleanCategory`), hidden unless `sweep diagnose --deep` / `sweep clean --deep`.
- **F6 Service-aware unlock (opt-in):** `src/infra/win/service_lock.rs` `ServiceGuard {stop([wuauserv,bits,dosvc]), Drop→restore}`; `sweep clean --deep --stop-services -y` stops `wuauserv`+`bits` to trash `SoftwareDistribution\Download` safely, then restores. Guard never auto-stops services unless `guard --allow-service-stop`.
- **F3 Idle SSD guard:** `src/services/idle_service.rs` diffs two `snapshot_io` 60s apart → `IdleSsdOffender {pid,name,idle_mins,write_per_hour}` where `idle_secs>30m && write>100 MB/h && pid!=foreground`. `sweep idle [--top 10] [--idle-mins 30] [--min-write-mb 100]` table `PID | APP | IDLE | WRITE/h | RAM | REASON(CacheBloat/LogSpam)` + `sweep idle --clean-cache` reuses whitelisted `CleanService`.
- **F7 Full benchmark:** every `clean`/`guard` logs `before/after free` + per-cat `removed_bytes`, `diagnose` shows `total reclaimable Safe X + System Y`.

**Ship check:** `sweep guard --once` trims on `RAM≥90%` synthetic, `sweep diagnose --deep` estimates WU+DO+WinSxS, `sweep idle` flags idle `chrome Code` renderers writing `>100 MB/h`, `sweep clean --deep --scan-only` never deletes system without flag.

**Fixes together:** Guard (F2 auto-trim) + Reserve (F1) ensures system never hits `0`; Deep (F4) + Service lock (F6) gives Optimizer parity without unsafe deletes; Idle (F3) + Benchmark (F7) proves reclaim.

---

## Stage 3 — Trust & Control (2–4 days) — fixes remaining F2+F3 control, undo, and distribution

**Goal:** user trusts unattended `guard`; can leave exclusions, undo, and kill only when they say so (constitution `v1.1.0` tier 2/3).

**Delivers:**
- **Doctor + Exclusions** (`ROADMAP.md:45-46`): `sweep doctor` pre-flight — elevation? toast available? `reserve ok/consumed`? `guard would trim [..] would clean [..]` sizes. `sweep.toml` exclusions honored everywhere (`services/clean_service.rs:60` + `dev_caches.rs` + `deep_clean.rs`).
- **Undo journal** (`ROADMAP.md:47`): `sweep undo` records trashed items per session (Recycle Bin restore limitation documented: purged = gone).
- **Controlled kill** (constitution `II` tier 2→3): `sweep idle --close` (`WM_CLOSE`), `sweep idle --kill --force` / `sweep bg --kill` requires explicit `confirm("kill chrome.exe PID 16648 232 MiB?")`, blocklist `PID 0/4, csrss, wininit`, log consent. Guard respects `guard --allow-kill` only then does graceful close for idle offenders writing `>500 MB/h` for `60m`. TUI `tui.rs:25` adds `b` bg view, `i` idle view, `k` kill (with confirm modal).
- **Cleaner rule packs** (`ROADMAP.md:48`): TOML `[[category]] id="myapp-cache" roots=["%LOCALAPPDATA%/MyApp/Cache"] risk="Safe"` — no code for new apps.
- **Distribution polish** (`ROADMAP.md:53`): `Cargo.toml` `[profile.release]` `strip/lto/codegen-units` (shrink `~7.8 MB`), `sweep --version` check, winget/scoop. Backlog items (`watch-folders`, `self-uninstall`, large/old-file wizard) stay out of scope.

**Ship check:** `sweep doctor` reports `reserve: ok, guard: armed, idle: 2 offenders`, `sweep idle --close` closes idle renderer gracefully, `--kill` requires `--force`+confirm, `sweep.toml` exclude respected by `diagnose`/`clean`/`guard`, CI `ubuntu+windows` green, `v0.9.0` tag attaches `sweep-linux-x64` (`release.yml:1`).

---

## Cross-cutting

- **Safety:** Safe cats → Recycle Bin; System cats → scan-only + `--deep`+`--stop-services`; kill path `trim → close → kill` with `confirm()` + audit `guard.log`. Mirrors Optimizer skill `safely stopping services` + `safety-first` claim.
- **Frugality:** `guard` ~0 CPU while healthy, `idle` 60s diff only on demand, index stays incremental (`walker.rs:104` `pause_every_dirs 25`, DB-seeded skip `197`), single binary.
- **Tests:** reserve lifecycle, dev cache discovery, `idle`/`deep` whitelisting, service guard stop/restore mocked, kill blocklist, `diagnose` rollup — all `#[ignore]` live probes kept.

**Version:** 1.0 | **Created:** 2026-08-25 | **Constitution:** `1.1.0` | **Next:** stage pick → `specify`→`plan`→`tasks`→implement.

