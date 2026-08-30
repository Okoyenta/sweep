# Quickstart: Trust & Control (Stage 3)

**Feature**: `specs/003-trust-control` | **Date**: 2026-08-29

End-to-end validation guide for the Stage 3 features. Run from repo root after building. References: spec (`spec.md`), data model (`data-model.md`), CLI contract (`contracts/cli.md`), research (`research.md`).

> Prerequisites: Rust 1.98+; on Windows run from a normal (non-elevated) prompt unless a step says "elevated". Build: `cargo build` (debug) or `cargo build --release` (tuned profile per FR-018).

---

## Q1 — `sweep doctor` pre-flight (FR-001..003, SC-001)

```text
cargo run -- doctor
```

**Expected**: prints `reserve: ok|missing|consumed`, `elevation: elevated|not`, `toast: available|unavailable`, `guard: armed|not armed`, `idle: <N> offenders`, and a `would-clean` total with per-category lines. Exit 0.

## Q2 — Exclusions honored (FR-004, FR-005, SC-002)

Create `sweep.toml` in CWD:
```toml
[exclusions]
category_ids = ["dev-pnpm"]
paths = ["C:/some/real/cache"]
```
```text
cargo run -- diagnose
cargo run -- clean --scan-only
```
**Expected**: `dev-pnpm` (and the excluded path) do NOT appear in diagnose/clean output; a line noting `excluded: N` is logged. Remove `sweep.toml` → items reappear.

## Q3 — Undo journal (FR-006..008, SC-003)

```text
cargo run -- clean -y --only user-temp      # moves items to Recycle Bin, writes session
cargo run -- undo                            # restores them
```
**Expected**: items return to original paths. Then purge the Recycle Bin and run `cargo run -- undo` again → reports `unrecoverable (recycle bin purged)`, exit 0.

## Q4 — Controlled kill (FR-009..013, SC-004, SC-005)

Launch a test idle app (e.g. `notepad`), let it idle.
```text
cargo run -- idle --close --only <pid>       # graceful close
cargo run -- idle --kill --force --only <pid>
```
**Expected**: `--close` exits the app cleanly. `--kill --force` shows `confirm("kill notepad.exe PID <pid> <size>?")` and only kills after approval. Try with a blocklisted PID (e.g. 4 / `csrss`) → always skipped.

Guard graceful-only (SC-005):
```text
cargo run -- guard --once --allow-kill
```
**Expected**: on an idle offender writing >500 MB/h for >60m, guard performs graceful close only (logged), never forced kill.

## Q5 — TUI views (FR-014, SC-006)

```text
cargo run -- tui
```
**Expected**: press `b` → background list; `i` → idle-offender table (PID, APP, IDLE, WRITE/h, RAM, REASON); select + `k` → confirmation modal; respects blocklist.

## Q6 — Rule packs (FR-015, FR-016, SC-007)

Add to `sweep.toml`:
```toml
[[category]]
id = "myapp-cache"
roots = ["%LOCALAPPDATA%/MyApp/Cache"]
risk = "Safe"
```
Pre-create the root with dummy files.
```text
cargo run -- clean --scan-only
```
**Expected**: `myapp-cache` appears with computed size + `Safe` risk. Change `risk = "System"` and run without `--deep` → hidden; with `--deep` → shown.

## Q7 — Distribution polish (FR-018, FR-019, FR-020, SC-008, SC-009)

```text
cargo build --release                      # tuned profile (strip/lto/codegen-units=1)
# measure binary size; expect < 7.8 MB baseline
cargo run -- --version                     # reports version; update hint when online
```
**Expected**: release binary smaller than ~7.8 MB; `--version` prints current and (online) an update hint. CI attaches `sweep-linux-x64` + winget/scoop manifests on `v*` tag (SC-009).

---

## CI gate (SC-010)

```text
cargo test --locked        # must pass on ubuntu-latest AND windows-latest
```

All quickstart scenarios should have a corresponding `#[test]` or `#[cfg(test)]` in the relevant `services/` module; live/OS-state probes are `#[ignore]`d on CI.
