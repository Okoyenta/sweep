# CLI Contract: sweep diagnose --deep & sweep clean --deep

**Commands**: `sweep diagnose --deep`, `sweep clean --deep`, `sweep clean --deep --stop-services`

**Purpose**: Discover and optionally clean Windows system bloat (Windows Update downloads, Delivery Optimization, WinSxS, driver store) that is normally hidden from standard scans.

## Usage

```
sweep diagnose [--deep]
sweep clean [--deep] [--stop-services] [--scan-only] [--only <category>] [-y]
```

## Behavior

### diagnose --deep

Extends standard diagnose output with System-risk categories:

1. Discover Windows Update downloads (`C:\Windows\SoftwareDistribution\Download`)
2. Discover Delivery Optimization cache (`C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache`)
3. Analyze WinSxS via `dism /Online /Cleanup-Image /AnalyzeComponentStore` (read-only; parse "Estimated Component Store Cleanup Needed: X bytes")
4. Discover driver store (`C:\Windows\System32\DriverStore\FileRepository`) — size + oldest driver date
5. Append to DiagnoseReport with `risk: System`

### clean --deep

Includes System-risk categories in the clean scan. Categories are listed but **not cleaned** unless `--stop-services` is also provided for locked categories.

- Without `--stop-services`: WU download category is listed in scan but skipped during clean (locked by wuauserv)
- With `--stop-services`: wuauserv + bits are stopped, WU downloads are trashed, services are restored

### clean --deep --scan-only

Reports sizes of all categories (Safe + System) without deleting anything. Same as diagnose but using the clean pipeline.

## Output Format

### diagnose --deep

```
  CATEGORY                          SIZE       RISK  RECLAIM
  npm-cache                      6.64 GiB     Safe    6.64 GiB
  cargo-cache                    1.20 GiB     Safe    1.20 GiB
  wu-downloads                   2.30 GiB   System         -
  delivery-optimization          890 MiB    System         -
  winsxs-reclaimable             450 MiB    System         -
  driver-store                   3.20 GiB   System         -
  ...

  potential reclaim: 9.29 GiB (Safe 9.29 GiB, System 6.84 GiB)
```

### clean --deep (without --stop-services)

```
  CATEGORY                          SIZE       RISK  RECLAIM
  npm-cache                      6.64 GiB     Safe    6.64 GiB
  cargo-cache                    1.20 GiB     Safe    1.20 GiB
  wu-downloads                   2.30 GiB   System    (locked)
  ...

  moving items to the recycle bin (7.84 GiB freed estimate)?
```
Note: `wu-downloads` shows `(locked)` instead of size in RECLAIM column.

### clean --deep --stop-services

```
  stopping services: wuauserv, bits...
  services stopped
  CATEGORY                          SIZE       RISK  RECLAIM
  npm-cache                      6.64 GiB     Safe    6.64 GiB
  cargo-cache                    1.20 GiB     Safe    1.20 GiB
  wu-downloads                   2.30 GiB   System    2.30 GiB
  ...

  moving items to the recycle bin (10.14 GiB freed estimate)?
```

After clean:
```
  services restored: wuauserv, bits
  before 45.2 GiB free → after 55.3 GiB free (freed 10.1 GiB) in 8.2s
```

### Benchmark Output (all clean/guard operations)

```
  before 45.2 GiB free → after 55.3 GiB free (freed 10.1 GiB) in 8.2s
    npm-cache: 6.6 GiB
    cargo-cache: 1.2 GiB
    wu-downloads: 2.3 GiB
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Fatal error (e.g., dism unavailable, service stop failed) |

## Stderr

Service stop/start failures go to stderr with explanation. Guard logs them to guard.log.
