# Contract: Space Reserve Behavior

**Scope**: `sweep status`, `sweep clean`, `sweep bin`, `sweep index` (implicit via `open_store_with_reserve`)

## Reserve File

| Property | Value |
|----------|-------|
| Path (Windows) | `%LOCALAPPDATA%\sweep\reserve.bin` |
| Path (Linux) | `~/.local/share/sweep/reserve.bin` |
| Size | 512 MB (`RESERVE_SIZE_BYTES`, `src/domain/models.rs`) |
| Type | Pre-allocated file — occupies the full size on disk while it exists |
| Created by | `try_recreate_reserve()` — only when `free_bytes >= RECREATION_THRESHOLD_BYTES` (1 GB) |
| Consumed by | `consume_reserve()` — deletes file, returns freed bytes |
| Re-created by | After successful `sweep clean` + `sweep bin --empty`, subject to the same 1 GB gate |

**The reserve occupies real space.** It is not sparse: `set_len` alone does not set
the NTFS sparse attribute, so the file consumes 512 MB from the moment it exists.
That occupancy is the point — a reserve occupying nothing would free nothing when
deleted. It also means holding it on a volume below `HEADROOM_THRESHOLD_BYTES` is
strictly harmful, which is what the creation gate exists to prevent.

### Thresholds

All three live in `src/domain/models.rs`, which is the source of truth; this table
cites them rather than restating numbers, because they answer three different
questions and must not be collapsed into one:

| Constant | Value | Question it answers |
|----------|-------|---------------------|
| `HEADROOM_THRESHOLD_BYTES` | 256 MB | Is free space low enough that the reserve must be released to make room for the next write? (compare: `should_consume`) |
| `RECREATION_THRESHOLD_BYTES` | 1 GB | Has free space recovered enough to re-arm the reserve? (compare: `should_recreate`) |
| guard's disk threshold (`ROADMAP.md`) | 2 GB | Should the guard daemon take corrective action? |

The gap between the first two is deliberate hysteresis. Re-arming the moment free
space clears the consume threshold would push the volume straight back under it,
oscillating a 512 MB file on every command.

**`ensure_reserve()` is private to `src/infra/paths.rs`** and must stay that way. It
allocates without consulting free space, so calling it from a command that reclaims
nothing hands back space a `clean` just freed. `tests/reserve_call_sites.rs` fails
if any other file names it.

## Commands Affected

### `sweep status`

1. Call `try_recreate_reserve()` — creates the reserve only if
   `free_bytes >= RECREATION_THRESHOLD_BYTES`. `status` reclaims nothing, so an
   unconditional create here would re-occupy space a `clean` had just freed.
2. Attempt `open_store_with_reserve()`:
   - Try `SqliteStore::open(&index_db_path())`
   - On disk-full error: `consume_reserve()` → retry open once
   - On retry failure: print RAM/disks/top-processes with notice `index: unavailable (disk full, reserve consumed — run sweep bin --empty)` — **do not bail**
3. Print status as normal

### `sweep clean`

1. Check `free_bytes_on_index_volume() < HEADROOM_THRESHOLD_BYTES` → `consume_reserve()` for headroom
2. Run `CleanService::run()` with empty-only fix applied
3. Print benchmark before/after
4. `try_recreate_reserve()` (re-create only when `free_bytes >= RECREATION_THRESHOLD_BYTES`)

### `sweep bin --empty`

1. Check `free_bytes_on_index_volume() < HEADROOM_THRESHOLD_BYTES` → `consume_reserve()` for headroom
2. Run `TrashBin::purge_all()`
3. Print benchmark before/after
4. `try_recreate_reserve()` (re-create only when `free_bytes >= RECREATION_THRESHOLD_BYTES`)

### `sweep index`

1. Call `try_recreate_reserve()` — same gate as above. Below
   `RECREATION_THRESHOLD_BYTES` the run proceeds without a reserve rather than
   re-occupying space the volume needs; a reserve that cannot be allocated is worse
   than none, because it also costs the free space the indexing run is about to need.
2. `--compact` runs `SqliteStore::compact()` (SQLite `VACUUM`) to return deleted
   pages to the filesystem, after checking free space covers the current index size.
   See the note below.
3. Proceed with index as normal

## Index growth

`DELETE` only returns pages to SQLite's internal freelist — the index file never
shrinks on its own, so an index that reads as empty can still occupy hundreds of
megabytes. That space is not reclaimable by `clean`, `bin --empty`, or any other
command; on a volume near full it is a standing block equal to or larger than the
reserve itself. `sweep index --compact` is the only path that returns it. `VACUUM`
rewrites the database into a temp file and swaps it in, so it needs free space
roughly equal to the current index — the command checks this up front and reports
the numbers rather than failing partway through the rewrite.

## Environment Variables

| Variable | Effect |
|----------|--------|
| `SWEEP_DB` | Overrides `index_db_path()`. Example: `SWEEP_DB=D:\sweep\index.db` |
| `LOCALAPPDATA` | Windows data dir base (fallback: `%USERPROFILE%\AppData\Local`) |
| `XDG_DATA_HOME` | Linux data dir base (fallback: `~/.local/share`) |
| `HOME` | Linux home dir fallback |

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Reserve file locked by AV | `consume_reserve()` returns `None`; log warning; operation continues without reserve |
| Reserve missing at 0 B | `consume_reserve()` returns `None`; status shows fallback message with manual recovery hint |
| `SWEEP_DB` path non-existent parent | `SqliteStore::open()` creates dirs; if drive missing, error message names the path |
| `free_bytes` unavailable | Treat as 0; conservatively attempt reserve consumption |
