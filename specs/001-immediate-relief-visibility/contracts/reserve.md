# Contract: Space Reserve Behavior

**Scope**: `sweep status`, `sweep clean`, `sweep bin`, `sweep index` (implicit via `open_store_with_reserve`)

## Reserve File

| Property | Value |
|----------|-------|
| Path (Windows) | `%LOCALAPPDATA%\sweep\reserve.bin` |
| Path (Linux) | `~/.local/share/sweep/reserve.bin` |
| Size | 512 MB (`536_870_912` bytes) |
| Type | Sparse file (zero disk usage until deleted) |
| Created by | `ensure_reserve()` — called on first `sweep status` or `sweep index` |
| Consumed by | `consume_reserve()` — deletes file, returns freed bytes |
| Re-created by | After successful `sweep clean` + `sweep bin --empty` if `free_bytes >= 1 GB` |

## Commands Affected

### `sweep status`

1. Call `ensure_reserve()` (create if missing)
2. Attempt `open_store_with_reserve()`:
   - Try `SqliteStore::open(&index_db_path())`
   - On disk-full error: `consume_reserve()` → retry open once
   - On retry failure: print RAM/disks/top-processes with notice `index: unavailable (disk full, reserve consumed — run sweep bin --empty)` — **do not bail**
3. Print status as normal

### `sweep clean`

1. Check `free_bytes_on_index_volume() < 256 MB` → `consume_reserve()` for headroom
2. Run `CleanService::run()` with empty-only fix applied
3. Print benchmark before/after
4. If clean succeeded and `free_bytes >= 1 GB`: `ensure_reserve()` (re-create)

### `sweep bin --empty`

1. Check `free_bytes_on_index_volume() < 256 MB` → `consume_reserve()` for headroom
2. Run `TrashBin::purge_all()`
3. Print benchmark before/after
4. If purge succeeded and `free_bytes >= 1 GB`: `ensure_reserve()` (re-create)

### `sweep index`

1. Call `ensure_reserve()` (create if missing) — reserve must exist before long indexing starts
2. Proceed with index as normal

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
