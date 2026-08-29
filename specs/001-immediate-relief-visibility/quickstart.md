# Quickstart Validation Guide: Stage 1

**Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Prerequisites

- Rust 1.98+ stable installed (`rustc --version`)
- `cargo test --locked` passes on current branch before changes
- Windows: `%LOCALAPPDATA%\sweep\` directory writable
- Linux: `~/.local/share/sweep/` directory writable

## Validation Scenarios

### V1: Reserve file lifecycle (FR-001, FR-002, FR-005)

**Setup**: Delete `%LOCALAPPDATA%\sweep\reserve.bin` (or `~/.local/share/sweep/reserve.bin`)

```
# Step 1: ensure_reserve creates the file
sweep status
# EXPECT: reserve.bin exists, size = 512 MB (536,870,912 bytes)
ls -la %LOCALAPPDATA%\sweep\reserve.bin    # Windows
ls -la ~/.local/share/sweep/reserve.bin   # Linux

# Step 2: consume_reserve deletes the file
# (Simulate by manually deleting reserve.bin, then running status on a volume with <256 MB free,
#  or by calling consume_reserve in a unit test)
cargo test reserve_consume -- --nocapture
# EXPECT: test passes, reserve.bin deleted, freed_bytes == 536_870_912

# Step 3: re-create after clean
sweep clean --only npm-cache -y
sweep bin --empty -y
# EXPECT: reserve.bin re-created if free >= 1 GB
ls -la %LOCALAPPDATA%\sweep\reserve.bin
```

### V2: Status at 0 B (FR-003)

**Setup**: Fill test volume to <256 MB free (or mock `free_bytes` to 0 in unit test)

```
sweep status
# EXPECT: RAM, disk free, top-processes printed
# EXPECT: notice: "index: unavailable (disk full, reserve consumed — run sweep bin --empty)"
# EXPECT: NO panic, NO hard error
```

**Unit test** (`cargo test status_fallback_at_0b`):
- Mock `SqliteStore::open()` to return disk-full error
- Mock `consume_reserve()` to return `Some(536_870_912)`
- Assert status output contains fallback notice

### V3: Clean headroom (FR-004)

**Setup**: Volume with <256 MB free, reserve.bin exists

```
sweep clean --only npm-cache -y
# EXPECT: reserve consumed first, then clean proceeds
# EXPECT: output includes "freed X" and before/after benchmark
```

### V4: Empty-only bug fix (FR-007)

```
# With no --only flag (empty vec):
sweep clean --scan-only
# EXPECT: all categories listed (not "nothing to clean")

# Unit test:
cargo test clean_empty_only_bug -- --nocapture
# EXPECT: CleanService::run(&scans, Some(&[])) cleans ALL categories, not zero
```

### V5: Dev cache discovery (FR-008, FR-009)

**Setup**: Create seed directories:

```bash
# Windows
mkdir %LOCALAPPDATA%\pnpm\store\test-cache
echo dummy > %LOCALAPPDATA%\pnpm\store\test-cache\file.bin

# Linux
mkdir -p ~/.local/share/pnpm/store/test-cache
echo dummy > ~/.local/share/pnpm/store/test-cache/file.bin

mkdir -p ~/.cargo/registry/cache/test-file
echo dummy > ~/.cargo/registry/cache/test-file
```

```
sweep clean --scan-only
# EXPECT: pnpm row with non-zero size
# EXPECT: cargo-cache row with non-zero size (if cargo caches exist)
# EXPECT: gradle-cache row (if ~/.gradle/caches exists)

cargo test dev_caches -- --nocapture
# EXPECT: discovery tests pass on both OS
```

### V6: Diagnose command (FR-012)

```
sweep diagnose
# EXPECT: table with CATEGORY | SIZE | RISK | RECLAIM columns
# EXPECT: sorted by SIZE descending
# EXPECT: summary line "potential reclaim: X (Safe X)"
```

**Empty state test**:
- Remove all cache directories
- `sweep diagnose` → "no cleanable categories found" + "potential reclaim: 0 B (Safe 0 B)"

### V7: Process I/O in snapshot (FR-010)

```
cargo test process_io_snapshot -- --nocapture
# EXPECT: SystemSnapshot top_processes contains at least one entry with write_bytes > 0
# EXPECT: on Linux, read_bytes/write_bytes default to 0 if /proc/[pid]/io unavailable
```

### V8: Benchmark before/after (FR-013)

```
sweep clean --only npm-cache -y
# EXPECT: line matching pattern "before X free → after Y free (freed Z) in Ns"
# EXPECT: Z = Y - X (within rounding tolerance)

sweep bin --empty -y
# EXPECT: same before/after pattern
```

### V9: SWEEP_DB fallback (FR-006)

```
SWEEP_DB=D:\sweep\index.db sweep status
# EXPECT: index.db created at D:\sweep\index.db
# EXPECT: status succeeds even if C: is full
```

### V10: Cross-platform CI gate (FR-015)

```
cargo test --locked
# EXPECT: all tests pass (0 failures, #[ignore] tests allowed)
cargo build --release
# EXPECT: binary builds successfully
```

## Expected Test Counts

After implementation, `cargo test --locked` should report:
- Existing tests: all passing (no regressions)
- New unit tests: ~8-12 (reserve lifecycle, empty-only, dev caches discovery, diagnose, benchmark)
- New integration tests: ~2-3 (reserve + clean end-to-end, diagnose output format)
- Total new LoC: ~400-500 across 6 new files + modifications to 8 existing files
