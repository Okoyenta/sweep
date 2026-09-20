# CLI Contract: Trust & Control (Stage 3)

**Feature**: `specs/003-trust-control` | **Date**: 2026-08-29

This contract documents the NEW and CHANGED command surface introduced by Stage 3. Existing commands (`status`, `index`, `clean`, `ram`, `dupes`, `guard`, `idle`, `schedule`) keep their current contracts; additions are listed below. All commands remain trash-backed and safe-by-default (Constitution Principle II).

> **Superseded by [Index provenance disclosure](#changed-command-status-index-dupes) below.** `status`
> and `index --status` each gain one line, and `status`'s process table renames a column. Those three
> changes are the only departures from the "keep their current contracts" rule above.

## New command: `sweep doctor`

Pre-flight safety report. No mutations.

```text
sweep doctor
```

**Output (stable fields)**:
```text
reserve: ok | missing | consumed | partial | below-headroom (<held> held, <free> free)
elevation: elevated | not
toast: available | unavailable
guard: armed | not armed
idle: <N> offenders
would-clean: <total-size> across <M> categories
  - <category-id>: <size> [Safe|System]
```

The `reserve:` status token keeps its position and is always followed by a
space-separated parenthetical reporting bytes held and bytes free, so a parser that
matches the prefix (`reserve: ok`) is unaffected. The status values are:

| Value | Meaning |
|-------|---------|
| `ok` | Full-size reserve on a volume with room to spare |
| `missing` | No reserve, and sweep has never run here |
| `consumed` | No reserve on a machine that has a data dir — released by an earlier rescue |
| `partial` | File present but under full size: an allocation that failed partway, not a deliberate release |
| `below-headroom` | Full-size reserve on a volume below `HEADROOM_THRESHOLD_BYTES`, i.e. held where it should be released. Size alone previously read as `ok`, which asserted a healthy posture at the one moment the reserve needed freeing. |

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

## Changed command: `status`, `index --status`, `dupes` — index provenance

Every answer derived from the index now states the index's age and how much of the disk it
covered. A bare `no duplicate groups found` against a 14-day-old index scoped to 11 % of the volume
is a clean negative that reads as a fact about the disk; this line is what separates the two.

```text
index: 14 days old, covers 24.14 GiB of C:\ (11%) — run `sweep index` for current results
```

- Printed by `status` (replacing the old `(last run: <unix-seconds>)` suffix), by
  `index --status`, and by `dupes` on **both** branches — a stale positive misleads the same way a
  stale negative does.
- `last run:` is now absolute *and* relative: `2026-08-22 22:47 UTC (14 days ago)`, or `never`.
  The bare unix timestamp is no longer emitted.
- Variants: `index: never built — run \`sweep index\` first`; coverage `(scope not recorded)` when
  the index predates scope recording; `(last run was interrupted — figures are partial)` in place of
  the `run \`sweep index\`` tail. Coverage below 1 % renders `<1%`, never `0%`.
- Denominators and non-disclosure: coverage is `indexed_bytes` over the summed capacity of the
  volumes the recorded roots live on (`of C:\` for one, `of N volumes` for several). It is a
  disclosure, not a precision claim — `IndexStats` counts readable bytes only, so a low figure may
  reflect locked directories rather than scope.
- `status`'s process table column `LAST RUN` is replaced by `STARTED`, the process's own start time
  (`UP`-style relative age). The old column was `unknown` for every row: it joined a usage map whose
  only unelevated source (UserAssist) excludes shell-external launches, so it read as a fact about
  each program when it was a fact about the probe.

**Exit codes**: unchanged.

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
