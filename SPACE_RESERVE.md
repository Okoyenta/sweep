# Space Reserve — Plan to keep sweep usable when C: hits 0

## Problem
`sweep status` fails at `0.00 B free` on `C:\` (`src/main.rs:53` `open_store()` → `SqliteStore::open(&index_db_path())` → SQLite `Error code 1546: disk I/O error`). Same for `cargo build --release` (`rustc-LLVM IO failure: No space left`).

Root cause:
- Index lives at `%LOCALAPPDATA%\sweep\index.db` (WAL) — `src/main.rs:492` / `src/infra/paths.rs:1`. SQLite needs to create `-wal`/`-shm`.
- `sweep clean -y` moves to Recycle Bin (`src/infra/trash_remover.rs:22` `trash::delete`) — same volume, so free stays `0` until `sweep bin --empty -y` (`src/main.rs:382` `purge_all`). On `2026-08-23` scan: `7.97 GiB` cleanable (`npm-cache 6.64 GiB` + caches), but `clean` without `empty` didn't free.

Goal: **pre-reserve 256–1024 MB so sweep can still open, scan, and trash when C: is full**, and prevent reaching `0` in the first place.

## Decision (confirmed)
- Defaults: reserve `512 MB` (recommended; 256 minimal, 1024 if you build on C: — `target/` needs GBs).
- Location: `%LOCALAPPDATA%\sweep\reserve.bin` (sparse file, created on first `sweep index` / `sweep status`).
- Behaviour: auto-delete the file when free `< 2 GB` (`ROADMAP.md:19` guard thresholds: RAM 90%, disk <2 GB, poll 30s `ROADMAP.md:22`) to instantly free `512 MB` for WAL + trash metadata; re-create after successful clean/empty.
- Fallback: if open still fails, `run_status` shows RAM/disk/top-procs without index stats instead of hard-failing.
- Optional: `SWEEP_DB` env / `--db-path D:\sweep\index.db` to relocate index to `D:` when `C:` is chronically full.

## Options considered

| Method | How | Pros | Cons |
|---|---|---|---|
| **A. Reserve file (chosen)** | `reserve.bin` pre-allocated, deleted on critical | Works after `0 B` if created before full; instant `0→512 MB`; proven Windows pattern | Must exist before crisis |
| B. Relocate index | `SWEEP_DB` to `D:` | Survives `C:` full | Needs second volume |
| C. SQLite fallback | Open read-only/in-memory on failure | `status` stays readable | Doesn't help `clean` |
| D. Guard threshold | `sweep guard` auto-clean before `0` (`ROADMAP.md:22` poll 30s) | Prevents `0` | Needs daemon running early |

Chosen: **A + D + tiny C** — reserve + guard prevents `0`, fallback keeps `status` usable.

## Implementation plan

### Phase 0 — Paths (no behaviour change)
- `src/infra/paths.rs:1` add:
  ```rust
  pub fn reserve_path() -> PathBuf { app_data().join("reserve.bin") }
  pub fn reserve_size_bytes() -> u64 { 512 * 1024 * 1024 }
  ```
- `src/infra/paths.rs` helpers: `ensure_reserve(mb)` (create sparse file via `File::create` + `set_len`), `consume_reserve() -> Option<u64>` (remove, return freed), `has_reserve() -> bool`, `free_bytes_on_index_volume() -> u64` (via `sysinfo_monitor`).

### Phase 1 — Status fallback (`src/main.rs:492` `run_status`)
```rust
fn open_store_with_reserve() -> Result<SqliteStore> {
    match open_store() {
        Ok(s) => Ok(s),
        Err(e) if is_disk_full(&e) => {
            if consume_reserve().is_some() { open_store() } else { Err(e) }
        }
        Err(e) => Err(e),
    }
}
```
If second open still fails → print `memory`/`disks`/`top processes` and `index: unavailable (disk full, reserve consumed — run sweep bin --empty)` instead of `bail`.

### Phase 2 — Clean needs headroom (`src/main.rs:235` Win / `275` Linux `run_clean`)
- Before `svc.run` / `trash::delete`, check `free_bytes_on_index_volume() < 256 MB` → `consume_reserve()` to allow trash metadata writes.
- Fix existing bug: `svc.run(&scans, Some(&only))` with empty `only` skips all — change to `if only.is_empty() { None } else { Some(&only) }` (`src/main.rs:235`, `275` `CleanService::run` at `src/services/clean_service.rs:54`).
- After successful `clean` + `bin --empty`, re-create reserve via `ensure_reserve`.

### Phase 3 — CLI
- `sweep reserve --status --size-mb 512 --recreate` (optional; or `sweep index` auto-ensures).
- `ROADMAP.md:50` Phase 2 `sweep doctor` will report `reserve: ok/missing/consumed`.

### Phase 4 — Guard integration (`ROADMAP.md:9` `sweep guard`)
- Guard loop checks `free < disk_min_gb` (`2 GB`) → first `consume_reserve()` to gain breathing room, then trash-backed clean of safe categories (`user-temp`, browser caches, `npm/pip` `README.md:29`), then if still low → `TrashBin::purge_all()` (`ROADMAP.md:17`).

## Edge cases
- Reserve missing at `0 B` (user never ran `index` before fill): requires manual `clean --only npm-cache -y` + `bin --empty -y` as on `2026-08-23` (3 items `_libvips/_npx/_cacache` → `1` remaining). Document manual recovery.
- AV/linker races (`README.md:74` `-j 2`): reserve helps `cargo build` but `target/` on `C:` still needs GBs — recommend `CARGO_TARGET_DIR=D:\cargo-target`.
- Two volumes: if `D:` exists, offer `SWEEP_DB=D:\sweep\index.db` via `index_db_path()` fallback.

## Verification
- Unit: `reserve` create/consume/idempotent (`src/infra/paths.rs`).
- Integration: simulate `0 B` by mocking `free_bytes` → `status` still prints, `clean` consumes reserve.
- CI: `ubuntu-latest` + `windows-latest` (`ci.yml:6`) both green (previous fix `E0716` `src/infra/schedule.rs:78` + `#[ignore]` probe `tests/live_usage_probe.rs:3` + fs-order test `src/infra/linux/apps.rs:191`).

## Rollout
1. Write this doc (done)
2. Patch `src/infra/paths.rs` + `src/main.rs:53`, `235`, `275`, `492`
3. `cargo test --locked` locally, push → CI green
4. Manual test at `0 B`: `sweep status`, `sweep clean -y`, `sweep bin --empty -y`, `sweep reserve --status`
5. Update `README.md:66` + `ROADMAP.md:39` Phase 2 docs

**Version**: 1.0 | **Created**: 2026-08-24 | **Next**: implement Phase 0–1 (awaiting go)
