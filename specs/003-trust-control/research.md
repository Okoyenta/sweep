# Research: Trust & Control (Stage 3)

**Feature**: `specs/003-trust-control` | **Date**: 2026-08-29

Resolves every NEEDS CLARIFICATION in the Technical Context. Each item records Decision / Rationale / Alternatives.

---

## R1. Where does `sweep.toml` live?

- **Decision**: Resolution order — (1) `./sweep.toml` in current directory, (2) platform user config dir: `%LOCALAPPDATA%/sweep/sweep.toml` on Windows, `~/.config/sweep/sweep.toml` on Linux. A `--config <path>` override takes highest precedence. If none exist, behave exactly as today (no exclusions).
- **Rationale**: Matches the existing reserved/state dir convention (`%LOCALAPPDATA%/sweep`) and lets power users keep a repo-local override. CWD-first supports project-specific excludes (e.g., a game repo).
- **Alternatives considered**: XDG-only (`~/.config` everywhere) — rejected because Windows users expect `%LOCALAPPDATA%`; env-var-only — rejected as less discoverable.

## R2. Undo journal storage & purge detection

- **Decision**: An append-only JSON file at `%LOCALAPPDATA%/sweep/undo.json` (Win) / `~/.local/share/sweep/undo.json` (Linux). Each entry: `{ session_id, timestamp, items: [{original_path, trash_path}] }`. `sweep undo` restores the newest session whose items still exist in the Recycle Bin; if a `trash_path` no longer resolves (Bin purged), it reports "unrecoverable" per item.
- **Rationale**: The `trash` crate already moves items to the OS Recycle Bin; the journal only needs to remember original↔trash mappings. JSON keeps it human-inspectable and avoids a schema migration in `index.db`.
- **Alternatives considered**: Storing in `index.db` (rusqlite) — possible but couples undo lifecycle to the index; ring-buffer in memory — rejected (must survive process exit).

## R3. `--version` update check source

- **Decision**: Query `https://api.github.com/repos/Okoyenta/sweep/releases/latest` with a 2-second timeout and a `User-Agent` header. Parse `tag_name`; compare semver to `CARGO_PKG_VERSION`. Offline / non-2xx / timeout → print current version only, no error.
- **Rationale**: The repo is already on GitHub; the Releases API is free, unauthenticated (with UA), and needs no publish-token. 2s timeout keeps `sweep --version` snappy and offline-safe.
- **Alternatives considered**: A custom hosted manifest — extra infra; crates.io API — lags behind GitHub tag releases used by `release.yml`.

## R4. TOML parsing dependency

- **Decision**: Add `toml = "0.8"` (serde-based). Define deserialization structs for `ExclusionConfig` and `RulePackCategory`. No other new deps.
- **Rationale**: Small, ubiquitous, zero-CVE history, already familiar. Avoids hand-rolled parser.
- **Alternatives considered**: `toml_edit` (preserves formatting for writes — not needed, we only read), `serdeconv` (redundant).

## R5. Elevation probe (doctor)

- **Decision**: Windows via `windows-sys` `GetTokenInformation(TokenElevation)` on the process token; Linux via `geteuid() == 0`. Report `elevated` / `not`.
- **Rationale**: No admin rights needed for the probe itself; token read is allowed for any process. Reuses the declaration-only FFI mandate in Principle I.
- **Alternatives considered**: Spawning `net session` / `whoami /groups` — heavier, localized strings, parsing fragility.

## R6. Toast availability probe (doctor)

- **Decision**: Probe by checking the PowerShell WinRT toast type is present (registry/`AppModel` capability) — reuse the same one-liner already used by guard; if it throws/unsupported, report `unavailable`. Doctor does not send a toast, only reports capability.
- **Rationale**: Single source of truth with guard's existing toast path; no duplicate implementation.
- **Alternatives considered**: A separate capability API — more FFI surface for no benefit.

## R7. Kill blocklist definition

- **Decision**: Hard-coded set: PIDs `0` and `4` (Windows System/Idle), process names `csrss`, `wininit`, `services`, plus sweep's own PID and any PID == current process. Applied in `kill_service` before any `WM_CLOSE`/`taskkill`.
- **Rationale**: Constitution Principle II explicitly names PID 0/4, csrss, wininit, services as never-kill. Adding self-PID prevents suicide.
- **Alternatives considered**: Configurable blocklist — unnecessary complexity; these are invariant system processes.

## R8. Graceful close vs kill mechanism

- **Decision**: `sweep idle --close` → `WM_CLOSE` (Win, `PostMessage`) / `SIGTERM` (Linux) with a short wait, then report. `sweep idle --kill --force` → `taskkill /F` (Win) / `SIGKILL` (Linux) ONLY after `confirm()` returns true. Guard `--allow-kill` uses only the graceful-close path (tier 2), never forced.
- **Rationale**: Graduated escalation mandated by Principle II. `confirm()` is already wired in `ui/guard.rs` for kill prompts.
- **Alternatives considered**: Always `taskkill /F` — violates safety-first; skipped.

## R9. Rule-pack vs exclusions file

- **Decision**: Rule packs and exclusions share ONE file: `sweep.toml`. Exclusions under `[exclusions]` (paths/category_ids/globs); custom cleaners under `[[category]]` (id, roots, risk). A `--rules <path>` flag can load an additional pack file.
- **Rationale**: One config surface is simpler for users; both are declarative TOML. Keeps FR-004 and FR-015 consistent.
- **Alternatives considered**: Separate `rules.toml` — extra file to discover; rejected for simplicity.

## R10. Release profile tuning

- **Decision**: Add to `Cargo.toml`: `[profile.release] strip = true; lto = true; codegen-units = 1; opt-level = "z"` (or "s"). Verify `cargo build --release --locked` on both OS in CI.
- **Rationale**: Constitution Principle I mandates a tuned release profile and CI verification. `opt-level="z"`/`"s"` plus `strip`+`lto` typically cuts the ~7.8 MB binary substantially.
- **Alternatives considered**: `panic = "abort"` — smaller but changes unwind semantics for guards; left default to stay safe.

---

All NEEDS CLARIFICATION resolved. Proceed to Phase 1 design.
