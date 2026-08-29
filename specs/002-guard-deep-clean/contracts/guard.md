# CLI Contract: sweep guard

**Command**: `sweep guard`

**Purpose**: Background daemon that monitors RAM and disk, automatically reclaiming resources before the system hangs.

## Usage

```
sweep guard [OPTIONS]

Options:
  --ram-threshold <N>    RAM usage % to trigger trim (default: 90)
  --disk-min-gb <N>      Minimum free disk GB before rescue (default: 2)
  --interval-secs <N>    Poll interval in seconds (default: 30)
  --once                 Run one poll cycle and exit
  --allow-service-stop   Allow guard to stop services for deep rescue (default: false)
  --allow-kill           Allow guard to kill processes (default: false)
  -h, --help             Print help
```

## Behavior

### Polling Loop

When `--once` is not set, guard enters an infinite loop:
1. Sleep for `interval_secs`
2. Snapshot RAM and disk
3. Check RAM pressure: if used% ≥ threshold for 3 consecutive samples → trim
4. Check disk pressure: if free < min_gb → disk rescue
5. If no pressure: continue loop (near-zero work)

### RAM Trim

When RAM pressure is confirmed (3 consecutive samples ≥ threshold):
1. Log "RAM pressure detected: X% used"
2. Call `RamService::optimize(top10)` — trim top-10 working sets
3. Call `purge_standby()` if on Windows
4. Display toast: "Sweep: trimmed top-10 processes, freed ~X MB"
5. Enter cooldown (10 minutes)

### Disk Rescue

When disk pressure is detected (free < min_gb):
1. Log "Disk pressure detected: X GB free"
2. Phase 1: `consume_reserve()` — free 512 MB sparse file
3. Check free space; if above threshold → log + toast + cooldown → done
4. Phase 2: Trash safe categories (user temp, browser caches, npm/pip)
5. Check free space; if above threshold → log + toast + cooldown → done
6. Phase 3: `TrashBin::purge_all()` — empty Recycle Bin
7. Log + toast + cooldown

### Cooldown

After any rescue action, guard enters cooldown for `cooldown_secs` (default 600s = 10 min). During cooldown, guard continues polling but skips all actions.

### Toast Notifications

Each rescue action triggers a Windows toast via PowerShell WinRT. Format:
```
Sweep Guard: [action summary]
  Freed: ~X MB
  Free space: Y GB
```
Graceful no-op if toast unavailable (logged to guard.log).

### Single-Instance

Guard acquires an exclusive file lock on `%LOCALAPPDATA%\sweep\guard.lock`. If lock acquisition fails, guard prints "guard is already running" and exits with code 1.

## Output Format

### stdout (interactive)

Guard runs silently in background. No stdout output unless `--once` is used:
```
polling every 30s (RAM≥90%, disk<2GB, cooldown=600s)
  [cycle 1] RAM 87%, disk 12.3 GB — OK
  [cycle 2] RAM 91%, disk 12.1 GB — OK
  [cycle 3] RAM 93%, disk 12.0 GB — RAM PRESSURE (3/3)
  trimming top-10 processes...
  freed ~245 MB (trim), standby purged
  toast sent
  entering cooldown (600s)
```

### guard.log

```
[2026-08-26T14:30:00Z] [ACTION] RAM trim: 10 PIDs trimmed, freed 245 MB, standby purged
[2026-08-26T14:30:00Z] [INFO] Toast sent: success
[2026-08-26T14:30:00Z] [INFO] Cooldown until 14:40:00
[2026-08-26T14:45:00Z] [ACTION] Disk rescue phase 1: consumed reserve, freed 512 MB
[2026-08-26T14:45:00Z] [ACTION] Disk rescue phase 2: trashed safe categories, freed 1.2 GB
[2026-08-26T14:45:00Z] [INFO] Free space now 3.7 GB — above threshold
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Graceful exit (signal, or `--once` completed) |
| 1 | Guard already running (mutex lock failed) |

## Stderr

Errors go to stderr via `anyhow` default handler. Toast failures are logged to guard.log, not stderr.
