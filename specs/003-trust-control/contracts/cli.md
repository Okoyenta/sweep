# CLI Contract: Trust & Control (Stage 3)

**Feature**: `specs/003-trust-control` | **Date**: 2026-08-29

This contract documents the NEW and CHANGED command surface introduced by Stage 3. Existing commands (`status`, `index`, `clean`, `ram`, `dupes`, `guard`, `idle`, `schedule`) keep their current contracts; additions are listed below. All commands remain trash-backed and safe-by-default (Constitution Principle II).

## New command: `sweep doctor`

Pre-flight safety report. No mutations.

```text
sweep doctor
```

**Output (stable fields)**:
```text
reserve: ok | missing | consumed
elevation: elevated | not
toast: available | unavailable
guard: armed | not armed
idle: <N> offenders
would-clean: <total-size> across <M> categories
  - <category-id>: <size> [Safe|System]
```

**Exit code**: 0 always (reporting only; never errors on missing reserve/guard).

## New command: `sweep undo`

Restore the most recent trashed session from the undo journal.

```text
sweep undo
```

**Behavior**:
- Restores newest `UndoSession` items to `original_path`.
- Reports per-item `restored` or `unrecoverable (recycle bin purged)`.
- No args. Exit 0 even when nothing to undo (prints "no session to undo").

## Changed command: `sweep clean`

Existing flags (`--scan-only`, `--only`, `-y`, `--deep`, `--stop-services`, `--kill`) unchanged. Semantics added:

- Honors `sweep.toml` `[exclusions]` (paths / category_ids / globs) — excluded items omitted from scan and clean, logged as `excluded`.
- Writes an `UndoSession` to the journal for every trashed item.

## Changed command: `sweep diagnose`

- Honors `sweep.toml` `[exclusions]`: excluded categories/paths removed from output.
- `--deep` shows `RulePackCategory` entries with `risk=System` hidden unless `--deep` (same policy as built-ins).

## Changed command: `sweep idle`

```text
sweep idle [--top N] [--idle-mins M] [--min-write-mb K] [--clean-cache]
sweep idle --close [--only <pid>...]
sweep idle --kill --force [--only <pid>...]
```

- `--close`: graceful `WM_CLOSE` / `SIGTERM` to idle offenders (tier 2).
- `--kill --force`: requires per-process `confirm("kill <name> PID <pid> <size>?")`; system-critical PIDs (0/4, csrss, wininit, services, self) always skipped.
- Blocklist applied before any action regardless of flags.

## New command: `sweep bg`

Background-process management view/CLI counterpart to TUI `b`.

```text
sweep bg [--top N] [--kill --force [--only <pid>...]]
```

Same kill consent + blocklist rules as `sweep idle --kill`.

## Changed command: `sweep guard`

- New optional flag `--allow-kill`: when set, guard may perform graceful **close** (tier 2) on idle offenders writing `>500 MB/h` for `>60m`. Never forced kill. Without the flag, guard is trim-only (unchanged).
- Honors `sweep.toml` `[exclusions]` during disk rescue.

## Changed command: `sweep --version` / top-level

```text
sweep --version
```

- Prints `sweep <CARGO_PKG_VERSION>`.
- When online, queries GitHub Releases (2s timeout); if a newer `tag_name` exists, appends `update available: <tag>`.
- Offline / timeout → prints version only, exit 0.

## Config file contract: `sweep.toml`

```toml
[exclusions]
paths = ["C:/Games/Cache"]
category_ids = ["dev-pnpm"]
globs = ["**/node_modules/**"]

[[category]]
id = "myapp-cache"
roots = ["%LOCALAPPDATA%/MyApp/Cache"]
risk = "Safe"            # or "System"
# cleanup_command = "myapp --clear"   # optional
```

Resolution order: `--config <path>` > `./sweep.toml` > user config dir. Missing/invalid → no exclusions, built-ins only (logged).

## Stability note

All new stdout fields above are part of the Stage 3 contract and may be parsed by scripts. Human-readable prose around them is non-contractual.
