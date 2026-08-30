# Implementation Plan: Trust & Control (Stage 3)

**Branch**: `main` | **Date**: 2026-08-29 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-trust-control/spec.md`

**Note**: This plan was produced by the `/speckit.plan` workflow (setup script replicated manually; bash unavailable on this Windows host).

## Summary

Stage 3 makes unattended `sweep guard` trustworthy. It adds a `sweep doctor` pre-flight check, per-user `sweep.toml` exclusions honored across diagnose/clean/guard, an `sweep undo` journal for Recycle-Bin recoverability, graduated consent-gated process termination (`--close` / `--kill --force` + confirm, system-critical blocklist, guard `--allow-kill` graceful-only), TUI `b`/`i`/`k` views, TOML cleaner rule packs, and distribution polish (tuned release profile, `--version` check, winget/scoop manifests).

## Technical Context

**Language/Version**: Rust 1.98+ stable (per constitution)

**Primary Dependencies**: `clap` (CLI), `ratatui` (TUI), `toml` (NEW — parse `sweep.toml` + rule packs), `sysinfo` (process/I/O), `trash` (Recycle Bin), `windows-sys` (Win32 FFI: `WM_CLOSE`, token/elevation check, `taskkill` via `std::process`), existing `rusqlite` (undo journal — reuse index DB or a small side table).

**Storage**: `sweep.toml` (user exclusions + rule packs) in `%LOCALAPPDATA%/sweep/sweep.toml` (Win) / `~/.config/sweep/sweep.toml` (Linux), with CWD override. Undo journal stored as a JSON/sledside file at `%LOCALAPPDATA%/sweep/undo.json` (or a table in `index.db`). No new DB engine.

**Testing**: `cargo test --locked`; cross-OS CI (`ubuntu-latest` + `windows-latest`). Live probes `#[ignore]`d.

**Target Platform**: Windows 10/11 (primary) + Linux (parity). `WM_CLOSE` is Win-only; Linux graceful close uses `SIGTERM`.

**Project Type**: single static CLI/TUI binary (monorepo crate `sweep`).

**Performance Goals**: doctor < 5s; guard idle RSS < 50 MB, CPU ~0%; undo restore of a session < 2s.

**Constraints**: < 50 MB RSS, single static binary, no Node/.NET runtime; trash-backed only; termination never silent (Principle II). Release binary must shrink from ~7.8 MB via `[profile.release]`.

**Scale/Scope**: 7 user stories; ~12–16 new/modified source files; adds 1 new crate dep (`toml`).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Resource Frugality | PASS | doctor is on-demand (no daemon); `toml` parse is trivial; release profile tuned per FR-018 reduces size. |
| II. Safety-First / Controlled Termination | PASS | Exclusions + undo add safety; kill is graduated (trim→close→kill), blocklist enforced, consent logged. Guard `--allow-kill` only does graceful close. |
| III. Strict Layered Architecture | PASS | New logic in `services/` (exclusions, undo, doctor, kill), `infra/` (win/linux impls), `ui/` (cli + tui). Domain models extended, no OS imports in `domain/`. |
| IV. Test-First & Verification | PASS | Each FR maps to a test; CI green on both OS required (SC-010). |
| V. Cross-Platform Parity | PASS | Linux equivalents for close/elevation/rule-pack loading defined. |
| VI. Observability & Trust | PASS | doctor + undo + confirm prompts directly serve this principle. |
| VII. Self-Documenting Code | PASS | FR-021 mandates doc comments on new public items. |

No gate violations. Complexity Tracking not required.

## Project Structure

### Documentation (this feature)

```text
specs/003-trust-control/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (CLI contract)
│   └── cli.md
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root) — additions/changes over existing `src/`

```text
src/
├── domain/models.rs            # + DoctorReport, ExclusionConfig, UndoJournal, KillRequest, RulePackCategory
├── domain/traits.rs            # + Exclusions port, UndoJournal port (optional)
├── infra/
│   ├── win/
│   │   ├── doctor.rs           # NEW: elevation + toast probe, guard-state read
│   │   ├── process_lock.rs     # EXTEND: add graceful_close(pid), kill(pid) gated by blocklist
│   │   └── paths.rs            # EXTEND: sweep.toml location
│   ├── linux/
│   │   ├── doctor.rs           # NEW: euid check, (toast n/a)
│   │   └── process_lock.rs     # EXTEND: SIGTERM close, SIGKILL gated
│   ├── exclusions.rs           # NEW: load + apply sweep.toml exclusions (cross-platform)
│   ├── rulepack.rs             # NEW: parse TOML [[category]]
│   └── undo.rs                 # NEW: write/read undo journal
├── services/
│   ├── doctor_service.rs       # NEW
│   ├── exclusion_service.rs    # NEW (applies exclusions in clean/diagnose/guard)
│   ├── undo_service.rs         # NEW
│   ├── kill_service.rs         # NEW (close/kill + blocklist + confirm)
│   └── clean_service.rs        # EXTEND: honor exclusions
├── ui/
│   ├── cli.rs                  # EXTEND: doctor, undo, idle --close/--kill flags, bg --kill
│   ├── diagnose.rs             # EXTEND: honor exclusions
│   └── tui.rs                  # EXTEND: b / i / k views + confirm modal
└── main.rs                     # EXTEND: route new subcommands
```

**Structure Decision**: Single crate, layered per Principle III. New `services/*_service.rs` hold orchestration; `infra/*` hold OS-specific FFI/IO; `ui/*` only parse/format. No new top-level dirs.

## Research (Phase 0)

See [research.md](./research.md) — all NEEDS CLARIFICATION resolved:

- `sweep.toml` resolution order: CWD → user config dir (with platform fallback).
- Undo journal format: append-only JSON file keyed by session timestamp; Recycle Bin purge detection via comparing restoration success.
- Version check: GitHub Releases API (repo already on GitHub) with 2s timeout, offline-safe.
- TOML crate chosen over `serde-toml` specifics; `toml` 0.8.
- Elevation probe: Win32 `GetTokenInformation` (TokenElevation) via `windows-sys`; Linux `geteuid()==0`.
- Kill blocklist: hard-coded set {PID 0,4; names csrss, wininit, services, plus sweep's own PID}.

## Design (Phase 1)

- Data model: [data-model.md](./data-model.md)
- CLI contract: [contracts/cli.md](./contracts/cli.md)
- Validation guide: [quickstart.md](./quickstart.md)

## Complexity Tracking

Not required — no constitution violations.
