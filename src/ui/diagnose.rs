use std::io::Write;

use crate::domain::models::{DiagnoseReport, DiagnoseRow, RiskLevel};
use crate::domain::traits::IndexStore;

pub fn run_diagnose(store: &dyn IndexStore, deep: bool) -> anyhow::Result<()> {
    let report = build_report(store, deep)?;
    let mut out = std::io::stdout();
    print_report(&report, &mut out)?;
    Ok(())
}

fn build_report(store: &dyn IndexStore, deep: bool) -> anyhow::Result<DiagnoseReport> {
    use crate::infra::dev_caches::discover_dev_categories;
    use crate::infra::paths::index_db_path;

    let mut rows: Vec<DiagnoseRow> = Vec::new();

    let dev_cats = discover_dev_categories();
    for cat in &dev_cats {
        let mut total = 0u64;
        for root in &cat.roots {
            if let Ok(meta) = std::fs::metadata(root) {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(root);
                }
            }
        }
        if total > 0 {
            let hint = if cat.id == "dev-pnpm" {
                Some("pnpm uses hardlinks; sweep runs 'pnpm store prune' for safe reclaim".into())
            } else {
                None
            };
            rows.push(DiagnoseRow {
                category_id: cat.id.clone(),
                title: cat.title.clone(),
                size_bytes: total,
                risk: RiskLevel::Safe,
                reclaimable: true,
                hint,
            });
        }
    }

    if deep {
        let deep_cats = crate::infra::deep_clean::deep_scan();
        if deep_cats.wu_download_bytes > 0 {
            rows.push(DiagnoseRow {
                category_id: "wu-downloads".into(),
                title: "Windows Update downloads".into(),
                size_bytes: deep_cats.wu_download_bytes,
                risk: RiskLevel::System,
                reclaimable: true,
                hint: None,
            });
        }
        if deep_cats.do_cache_bytes > 0 {
            rows.push(DiagnoseRow {
                category_id: "do-cache".into(),
                title: "Delivery Optimization cache".into(),
                size_bytes: deep_cats.do_cache_bytes,
                risk: RiskLevel::System,
                reclaimable: true,
                hint: None,
            });
        }
        if let Some(reclaimable) = deep_cats.winsxs_reclaimable_bytes {
            if reclaimable > 0 {
                rows.push(DiagnoseRow {
                    category_id: "winsxs".into(),
                    title: "WinSxS reclaimable".into(),
                    size_bytes: reclaimable,
                    risk: RiskLevel::System,
                    reclaimable: true,
                    hint: None,
                });
            }
        }
        if deep_cats.driver_store_bytes > 0 {
            rows.push(DiagnoseRow {
                category_id: "driver-store".into(),
                title: "Driver Store".into(),
                size_bytes: deep_cats.driver_store_bytes,
                risk: RiskLevel::System,
                reclaimable: false,
                hint: Some("Elevated: dism /Online /Cleanup-Image /StartComponentCleanup".into()),
            });
        }
    }

    if index_db_path().exists() {
        let stats = store.stats()?;
        if stats.total_bytes > 0 {
            rows.push(DiagnoseRow {
                category_id: "index".into(),
                title: "Index DB".into(),
                size_bytes: index_db_path()
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(0),
                risk: RiskLevel::System,
                reclaimable: false,
                hint: Some("Managed by sweep; use 'sweep index --full' to rebuild".into()),
            });
        }
    }

    rows.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    let total_reclaimable: u64 = rows
        .iter()
        .filter(|r| r.reclaimable)
        .map(|r| r.size_bytes)
        .sum();

    let safe_reclaimable: u64 = rows
        .iter()
        .filter(|r| r.reclaimable && r.risk == RiskLevel::Safe)
        .map(|r| r.size_bytes)
        .sum();

    let system_reclaimable: u64 = rows
        .iter()
        .filter(|r| r.reclaimable && r.risk == RiskLevel::System)
        .map(|r| r.size_bytes)
        .sum();

    Ok(DiagnoseReport {
        rows,
        total_reclaimable,
        safe_reclaimable,
        system_reclaimable,
        idle: None,
    })
}

fn print_report(report: &DiagnoseReport, w: &mut impl Write) -> std::io::Result<()> {
    use crate::ui::status::fmt;

    writeln!(
        w,
        "{:<18} {:>10} {:<8} {:<10}",
        "Category", "Size", "Risk", "Reclaim"
    )?;
    writeln!(w, "{}", "-".repeat(50))?;
    for row in &report.rows {
        let risk = match row.risk {
            RiskLevel::Safe => "Safe",
            RiskLevel::System => "System",
        };
        let reclaim = if row.reclaimable { "Yes" } else { "No" };
        writeln!(
            w,
            "{:<18} {:>10} {:<8} {:<10}",
            row.title,
            fmt(row.size_bytes),
            risk,
            reclaim
        )?;
        if let Some(ref hint) = row.hint {
            writeln!(w, "  -> {}", hint)?;
        }
    }
    writeln!(w, "{}", "-".repeat(50))?;
    if report.system_reclaimable > 0 {
        writeln!(
            w,
            "potential reclaim: {} (Safe {}, System {})",
            fmt(report.total_reclaimable),
            fmt(report.safe_reclaimable),
            fmt(report.system_reclaimable)
        )?;
    } else {
        writeln!(
            w,
            "potential reclaim: {} (Safe {})",
            fmt(report.total_reclaimable),
            fmt(report.safe_reclaimable)
        )?;
    }
    Ok(())
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_prints_rollup_zero() {
        let report = DiagnoseReport {
            rows: vec![],
            total_reclaimable: 0,
            safe_reclaimable: 0,
            system_reclaimable: 0,
            idle: None,
        };
        let mut buf = Vec::new();
        print_report(&report, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("potential reclaim: 0.00 B (Safe 0.00 B)"));
    }

    #[test]
    fn sorts_rows_by_size_desc_in_output() {
        let report = DiagnoseReport {
            rows: vec![
                DiagnoseRow {
                    category_id: "b".into(),
                    title: "large".into(),
                    size_bytes: 5000,
                    risk: RiskLevel::Safe,
                    reclaimable: true,
                    hint: None,
                },
                DiagnoseRow {
                    category_id: "a".into(),
                    title: "small".into(),
                    size_bytes: 100,
                    risk: RiskLevel::Safe,
                    reclaimable: true,
                    hint: None,
                },
            ],
            total_reclaimable: 5100,
            safe_reclaimable: 5100,
            system_reclaimable: 0,
            idle: None,
        };
        let mut buf = Vec::new();
        print_report(&report, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let large_idx = lines.iter().position(|l| l.contains("large")).unwrap();
        let small_idx = lines.iter().position(|l| l.contains("small")).unwrap();
        assert!(large_idx < small_idx);
    }

    #[test]
    fn column_alignment_matches_header() {
        let report = DiagnoseReport {
            rows: vec![DiagnoseRow {
                category_id: "x".into(),
                title: "cargo cache".into(),
                size_bytes: 1024,
                risk: RiskLevel::Safe,
                reclaimable: true,
                hint: None,
            }],
            total_reclaimable: 1024,
            safe_reclaimable: 1024,
            system_reclaimable: 0,
            idle: None,
        };
        let mut buf = Vec::new();
        print_report(&report, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].starts_with("Category"));
        assert!(lines[2].contains("cargo cache"));
        assert!(lines[2].contains("Safe"));
        assert!(lines[2].contains("Yes"));
    }
}
