use std::io::Write;

use crate::domain::categories::lookup;
use crate::domain::models::{DeepScanResult, DiagnoseReport, DiagnoseRow, RiskLevel};
use crate::domain::traits::IndexStore;

pub fn run_diagnose(
    store: &dyn IndexStore,
    deep: bool,
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let report = build_report(store, deep, config, rules)?;
    let mut out = std::io::stdout();
    print_report(&report, &mut out)?;
    Ok(())
}

fn build_report(
    store: &dyn IndexStore,
    deep: bool,
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> anyhow::Result<DiagnoseReport> {
    use crate::infra::dev_caches::discover_dev_categories;
    use crate::infra::paths::index_db_path;
    use crate::services::exclusion_service;

    let mut rows: Vec<DiagnoseRow> = Vec::new();

    // Same policy source as clean/guard, so an exclusion hides a category
    // everywhere and a rule pack shows up everywhere (FR-005, FR-015).
    let (exclusions, packs) = exclusion_service::load_policy_with_rules(config, rules);
    let dev_cats = discover_dev_categories();
    let mut all_cats = dev_cats.clone();
    all_cats.extend(exclusion_service::rule_packs_to_categories(
        &packs, &dev_cats, deep,
    ));

    for cat in &all_cats {
        let mut total = 0u64;
        for root in &cat.roots {
            if exclusion_service::is_path_excluded(root, &exclusions) {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(root) {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(root);
                }
            }
        }
        if total > 0 {
            rows.push(DiagnoseRow {
                category_id: cat.id.clone(),
                title: cat.title.clone(),
                size_bytes: total,
                risk: cat.risk,
                reclaimable: cat.reclaimable,
                hint: lookup(&cat.id).and_then(|d| d.hint).map(Into::into),
            });
        }
    }

    if deep {
        rows.extend(deep_rows(&crate::infra::deep_clean::deep_scan()));
    }

    if index_db_path().exists() {
        let stats = store.stats()?;
        if stats.total_bytes > 0 {
            let size = index_db_path().metadata().map(|m| m.len()).unwrap_or(0);
            rows.push(registry_row("index", size));
        }
    }

    let (mut rows, excluded) =
        crate::services::diagnose_service::filter_excluded_rows(rows, &exclusions);
    crate::ui::clean::print_excluded(excluded);

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

/// A row whose id, title, risk, reclaimability and hint all come from the
/// category registry, so diagnose and clean cannot disagree about it.
fn registry_row(id: &str, size_bytes: u64) -> DiagnoseRow {
    let d = lookup(id).unwrap_or_else(|| panic!("category '{id}' is not in the registry"));
    DiagnoseRow {
        category_id: d.id.into(),
        title: d.title.into(),
        size_bytes,
        risk: d.risk,
        reclaimable: d.reclaimable,
        hint: d.hint.map(Into::into),
    }
}

/// Rows for the deep (system) categories that have something to report.
fn deep_rows(deep: &DeepScanResult) -> Vec<DiagnoseRow> {
    [
        ("wu-downloads", deep.wu_download_bytes),
        ("do-cache", deep.do_cache_bytes),
        ("winsxs", deep.winsxs_reclaimable_bytes.unwrap_or(0)),
        ("driver-store", deep.driver_store_bytes),
    ]
    .into_iter()
    .filter(|&(_, bytes)| bytes > 0)
    .map(|(id, bytes)| registry_row(id, bytes))
    .collect()
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

    fn all_deep() -> DeepScanResult {
        DeepScanResult {
            wu_download_bytes: 1,
            do_cache_bytes: 1,
            winsxs_reclaimable_bytes: Some(1),
            driver_store_bytes: 1,
            driver_store_oldest_days: None,
        }
    }

    #[test]
    fn deep_rows_take_verdicts_from_registry() {
        let rows = deep_rows(&all_deep());
        assert_eq!(rows.len(), 4);
        for row in &rows {
            let d = lookup(&row.category_id).unwrap();
            assert_eq!(row.reclaimable, d.reclaimable, "{}", row.category_id);
            assert_eq!(row.risk, d.risk, "{}", row.category_id);
        }
        let ds = rows.iter().find(|r| r.category_id == "driver-store").unwrap();
        assert!(!ds.reclaimable);
        assert!(ds.hint.as_deref().unwrap().contains("dism"));
    }

    /// Regression for clean-diagnose-divergence: every category clean can
    /// discover carries the reclaimable verdict diagnose reports for that id,
    /// and clean never offers a report-only category.
    #[test]
    fn clean_categories_match_diagnose_verdicts() {
        use crate::services::clean_service::discover_with_policy;

        let deep_diag = deep_rows(&all_deep());
        for deep in [false, true] {
            for cat in discover_with_policy(None, None, deep).categories {
                if let Some(d) = lookup(&cat.id) {
                    assert_eq!(cat.reclaimable, d.reclaimable, "{}", cat.id);
                }
                if let Some(row) = deep_diag.iter().find(|r| r.category_id == cat.id) {
                    assert_eq!(cat.reclaimable, row.reclaimable, "{}", cat.id);
                }
                assert!(cat.reclaimable, "clean offers report-only {}", cat.id);
            }
        }
    }
}
