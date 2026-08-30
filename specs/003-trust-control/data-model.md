# Data Model: Trust & Control (Stage 3)

**Feature**: `specs/003-trust-control` | **Date**: 2026-08-29

Entities extracted from the feature spec (`spec.md`) and research (`research.md`). Validation rules map to FR-xxx; state transitions where applicable.

## Entities

### DoctorReport
Pre-flight snapshot produced by `sweep doctor`.
- `reserve_status: enum { Ok, Missing, Consumed }`
- `elevation: enum { Elevated, Not }`
- `toast: enum { Available, Unavailable }`
- `guard_armed: bool`
- `would_clean: Vec<CategoryEstimate>` — id, size_bytes, risk
- `would_clean_total_bytes: u64`
- `idle_offender_count: u64`

**Validation**: all fields populated; `would_clean_total_bytes` == sum of `would_clean[].size_bytes`.

### ExclusionConfig
Parsed from `sweep.toml` `[exclusions]`.
- `paths: Vec<PathBuf>`
- `category_ids: Vec<String>`
- `globs: Vec<String>`

**Validation**: missing file → empty config (no exclusions). Invalid TOML → log error, empty config (don't crash). Globs matched case-insensitively on path.

**State**: immutable per run; loaded once at startup.

### RulePackCategory
User-supplied cleaner from `[[category]]` in `sweep.toml` (or `--rules` file).
- `id: String` (unique; conflicts with built-in id → skipped with warning)
- `roots: Vec<PathBuf>`
- `risk: enum { Safe, System }`
- `cleanup_command: Option<String>` (optional; mirrors built-in dev-pnpm pattern)

**Validation**: `id` non-empty; at least one existing root (missing roots skipped silently, like built-ins). `risk=System` → hidden unless `--deep`.

### UndoJournal
Append-only record at `%LOCALAPPDATA%/sweep/undo.json`.
- `sessions: Vec<UndoSession>`
- `UndoSession { session_id: String, timestamp: u64, items: Vec<UndoItem> }`
- `UndoItem { original_path: PathBuf, trash_path: PathBuf }`

**Validation**: JSON parse failure → start fresh journal (warn). `sweep undo` restores only the newest `UndoSession`. Item recoverable iff `trash_path` still exists in Recycle Bin; otherwise reported unrecoverable.

**State transition**:
`clean writes session` → `undo restores (or reports purged)` → session retained for audit (optional prune by count).

### KillRequest
A requested termination.
- `pid: u32`
- `name: String`
- `size_bytes: u64`
- `mode: enum { Close, Kill }`
- `consent: bool`

**Validation / blocking**: `is_blocked(pid, name)` returns true for PID 0/4, names {csrss, wininit, services}, or self PID → request rejected before any action. `mode=Kill` requires `consent == true` (from `confirm("kill <name> PID <pid> <size>?")`).

### TuiView
TUI navigation state.
- `current: enum { Background, Idle, KillModal }`
- `selected_pid: Option<u32>`

**State**: `b` → Background; `i` → Idle; selecting item + `k` → KillModal (requires confirm to act).

## Relationships

- `ExclusionConfig` + `RulePackCategory` are both sourced from the same `sweep.toml` (R9).
- `CleanCategory` (existing) is filtered by `ExclusionConfig` before sizing; `RulePackCategory` is merged into `CleanCategory` list during discovery.
- `DoctorReport.would_clean` = `discover_categories()` minus `ExclusionConfig`, with `RulePackCategory` included.
- `UndoJournal` is written by `clean_service` (post-trash) and read by `undo_service`.
- `KillRequest` is produced by `idle`/`bg`/`guard` and validated by `kill_service` against the blocklist + consent.

## Validation Rules Summary (from FR)

- FR-005: exclusions prune before size calc, logged as excluded.
- FR-007/008: undo restores newest session; detects purged Bin.
- FR-011: blocklist always wins.
- FR-012: guard `--allow-kill` only `mode=Close`.
- FR-016: custom category risk/visibility == built-ins.
