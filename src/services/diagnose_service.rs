use crate::domain::models::{CategoryScan, DiagnoseReport, DiagnoseRow, RiskLevel};

pub struct DiagnoseService;

impl DiagnoseService {
    pub fn build_report(scans: &[CategoryScan]) -> DiagnoseReport {
        let mut rows: Vec<DiagnoseRow> = scans
            .iter()
            .map(|s| DiagnoseRow {
                category_id: s.category_id.clone(),
                title: s.title.clone(),
                size_bytes: s.total_bytes,
                risk: RiskLevel::Safe,
                reclaimable: true,
                hint: None,
            })
            .collect();

        rows.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

        let total_reclaimable: u64 = rows.iter().map(|r| r.size_bytes).sum();

        DiagnoseReport {
            rows,
            total_reclaimable,
            safe_reclaimable: total_reclaimable,
            system_reclaimable: 0,
            idle: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::CategoryScan;

    #[test]
    fn sorts_by_size_desc() {
        let scans = vec![
            CategoryScan {
                category_id: "small".into(),
                title: "small".into(),
                items: vec![],
                total_bytes: 100,
                files: 1,
                cleanup_command: None,
            },
            CategoryScan {
                category_id: "large".into(),
                title: "large".into(),
                items: vec![],
                total_bytes: 1000,
                files: 1,
                cleanup_command: None,
            },
            CategoryScan {
                category_id: "medium".into(),
                title: "medium".into(),
                items: vec![],
                total_bytes: 500,
                files: 1,
                cleanup_command: None,
            },
        ];
        let report = DiagnoseService::build_report(&scans);
        assert_eq!(report.rows[0].category_id, "large");
        assert_eq!(report.rows[1].category_id, "medium");
        assert_eq!(report.rows[2].category_id, "small");
    }

    #[test]
    fn sums_total_reclaimable() {
        let scans = vec![
            CategoryScan {
                category_id: "a".into(),
                title: "a".into(),
                items: vec![],
                total_bytes: 100,
                files: 1,
                cleanup_command: None,
            },
            CategoryScan {
                category_id: "b".into(),
                title: "b".into(),
                items: vec![],
                total_bytes: 200,
                files: 1,
                cleanup_command: None,
            },
        ];
        let report = DiagnoseService::build_report(&scans);
        assert_eq!(report.total_reclaimable, 300);
    }

    #[test]
    fn empty_input_returns_empty_report() {
        let report = DiagnoseService::build_report(&[]);
        assert!(report.rows.is_empty());
        assert_eq!(report.total_reclaimable, 0);
    }

    #[test]
    fn marks_all_rows_safe_and_reclaimable() {
        let scans = vec![CategoryScan {
            category_id: "x".into(),
            title: "x".into(),
            items: vec![],
            total_bytes: 50,
            files: 1,
            cleanup_command: None,
        }];
        let report = DiagnoseService::build_report(&scans);
        assert_eq!(report.rows[0].risk, RiskLevel::Safe);
        assert!(report.rows[0].reclaimable);
    }
}
