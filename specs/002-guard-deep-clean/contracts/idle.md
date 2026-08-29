# CLI Contract: sweep idle

**Command**: `sweep idle`

**Purpose**: Detect background processes that are idle but writing heavily to disk, identifying cache bloat, log spam, or misbehaving apps.

## Usage

```
sweep idle [OPTIONS]

Options:
  --top <N>           Maximum number of results to show (default: 20)
  --idle-mins <N>     Minimum idle time in minutes to flag (default: 30)
  --min-write-mb <N>  Minimum write rate in MB/hour to flag (default: 100)
  --clean-cache       Clean whitelisted cache for flagged processes
  -h, --help          Print help
```

## Behavior

### Detection

1. Take I/O snapshot of all processes (via `sysinfo` crate)
2. Record foreground PID (via `GetForegroundWindow` on Windows, or current TTY process on Linux)
3. Sleep 60 seconds
4. Take second I/O snapshot
5. For each process present in both snapshots:
   - Compute `write_delta = snap2.write_bytes - snap1.write_bytes`
   - Compute `idle_secs` (time since last user input on Windows via `GetLastInputInfo`, or process start time on Linux)
   - If `idle_secs > idle_mins * 60` AND `write_delta / 3600 > min_write_mb * 1024 * 1024` AND `pid != foreground_pid`:
     - Add to offenders list with reason classification

### Reason Classification

- **CacheBloat**: Process name matches known cache-heavy apps (chrome, code, slack, discord) OR write pattern is steady/sequential
- **LogSpam**: Write pattern is bursty/high-frequency (detected via delta variance across multiple snapshots — Stage 2 heuristic: if write > 500 MB/h, likely log spam)
- **Unknown**: Doesn't match known patterns

### Cache Cleaning

When `--clean-cache` is used:
1. For each flagged offender, identify its known cache directories (same logic as dev_caches.rs)
2. Trash items in those directories using `TrashRemover`
3. Report bytes freed per process

## Output Format

```
  PID    APP              IDLE    WRITE/h     RAM    REASON
 1234    chrome.exe       45m     180 MB/h   890 MB  CacheBloat
 5678    Code.exe          2h     120 MB/h   1.2 GB  CacheBloat
 9012    some-service.exe  3h     250 MB/h   45 MB   LogSpam

  3 offenders found (550 MB/h total writes)
```

### Empty State

```
  no idle heavy writers detected (thresholds: ≥30m idle, ≥100 MB/h write)
```

### Clean Cache Output

```
  cleaned chrome cache: 340 MB
  cleaned Code cache: 120 MB
  total freed: 460 MB
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (even if 0 offenders found) |
| 1 | Fatal error (e.g., cannot read process info) |

## Stderr

No stderr output under normal operation.
