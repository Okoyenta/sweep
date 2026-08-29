# CLI Contract: sweep diagnose

**Command**: `sweep diagnose`

**Purpose**: Read-only scan of all reclaimable categories with size, risk, and potential reclaim rollup. Includes idle-state probe on Windows.

## Usage

```
sweep diagnose
```

No flags in Stage 1. Future: `--deep` (System categories), `--json`, `--top N`.

## Output Format

```
  CATEGORY             SIZE       RISK  RECLAIM
  npm-cache         6.64 GiB     Safe    6.64 GiB
  cargo-cache       1.20 GiB     Safe    1.20 GiB
  pnpm                800 MiB    Safe    800 MiB
  gradle-cache        450 MiB    Safe    450 MiB
  user-temp           120 MiB    Safe    120 MiB
  chrome-cache         80 MiB    Safe     80 MiB
  ...

potential reclaim: 9.29 GiB (Safe 9.29 GiB)
```

### Table Columns

| Column | Width | Description |
|--------|-------|-------------|
| CATEGORY | 20 left-aligned | Category ID string |
| SIZE | 10 right-aligned | Human-readable size (binary units via `byte-unit`) |
| RISK | 6 right-aligned | `Safe` or `System` |
| RECLAIM | 10 right-aligned | Reclaimable size if Risk=Safe, else `-` |

### Summary Line

```
potential reclaim: <total> (Safe <safe_total>)
```

- `<total>` = sum of `size_bytes` where `risk == Safe`
- `<safe_total>` equals `<total>` in Stage 1 (no System categories)

### Empty State

```
no cleanable categories found

potential reclaim: 0 B (Safe 0 B)
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (even if 0 categories found) |
| 1 | Fatal error (e.g., cannot read disk info) |

## Stderr

No stderr output under normal operation. Errors go to stderr via `anyhow` default handler.
