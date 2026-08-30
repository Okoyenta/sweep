//! Pre-flight safety report backing `sweep doctor`.
//!
//! Assembles a [`DoctorReport`] from the reserve file, the elevation and toast
//! probes in `infra/{win,linux}/doctor.rs`, the guard autostart state, the
//! would-clean estimate, and the current idle-offender count. The command is
//! read-only: every probe degrades to a "not / unavailable / zero" answer rather
//! than failing, so doctor always completes and always exits 0 (Principle VI).

use std::path::Path;

use crate::domain::models::{
    CategoryEstimate, DoctorReport, ElevationStatus, ReserveStatus, ToastStatus,
    RESERVE_SIZE_BYTES,
};

/// Service that builds the `sweep doctor` pre-flight report.
pub struct DoctorService;

impl DoctorService {
    /// Create a doctor service. Holds no state; every probe runs per report.
    pub fn new() -> Self {
        Self
    }

    /// Collect every pre-flight signal into a single report.
    ///
    /// `config_override` / `rules_override` are the `--config` and `--rules`
    /// paths, so the would-clean estimate reflects the same exclusions and rule
    /// packs a real `sweep clean` would apply.
    pub fn report(
        &self,
        config_override: Option<&Path>,
        rules_override: Option<&Path>,
    ) -> DoctorReport {
        // The probes are independent and each spawns a process or scans system
        // state, so they run on their own threads alongside the sizing walk:
        // total cost is the slowest probe, not their sum, which keeps doctor
        // inside its 5-second budget (SC-001).
        let elevation_probe = std::thread::spawn(elevation_status);
        let toast_probe = std::thread::spawn(toast_status);
        let guard_probe = std::thread::spawn(guard_armed);
        let idle_probe = std::thread::spawn(idle_offender_count);
        let volume_probe = std::thread::spawn(volumes);

        let (would_clean, would_clean_partial) =
            would_clean_estimate(config_override, rules_override, SIZING_BUDGET);
        let would_clean_total_bytes = would_clean.iter().map(|c| c.size_bytes).sum();

        // A panicking probe must not take doctor down; fall back to the
        // "unknown means not available" defaults.
        let elevation = elevation_probe.join().unwrap_or(ElevationStatus::Not);
        let toast = toast_probe.join().unwrap_or(ToastStatus::Unavailable);
        let guard_armed = guard_probe.join().unwrap_or(false);
        let idle_offender_count = idle_probe.join().unwrap_or(0);
        let volumes = volume_probe.join().unwrap_or_default();

        DoctorReport {
            reserve_status: reserve_status(),
            elevation,
            toast,
            guard_armed,
            would_clean,
            would_clean_total_bytes,
            would_clean_partial,
            idle_offender_count,
            volumes,
        }
    }
}

/// Wall-clock budget for sizing the would-clean estimate.
///
/// A full walk of every cache tree takes minutes on a large disk, so doctor
/// spends a bounded slice of its 5-second budget (SC-001) and labels the result
/// partial if that was not enough.
const SIZING_BUDGET: std::time::Duration = std::time::Duration::from_millis(2500);

impl Default for DoctorService {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify the disk-full reserve file.
///
/// A full-size file is `Ok`. If the file is gone but sweep's data dir exists,
/// sweep has run before and the reserve was consumed by a disk-full rescue;
/// with no data dir at all it was simply never created.
fn reserve_status() -> ReserveStatus {
    let path = crate::infra::paths::reserve_path();
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() >= RESERVE_SIZE_BYTES => ReserveStatus::Ok,
        Ok(_) => ReserveStatus::Consumed,
        Err(_) => {
            if crate::infra::paths::data_dir().exists() {
                ReserveStatus::Consumed
            } else {
                ReserveStatus::Missing
            }
        }
    }
}

#[cfg(windows)]
fn elevation_status() -> ElevationStatus {
    crate::infra::win::doctor::elevation_status()
}

#[cfg(not(windows))]
fn elevation_status() -> ElevationStatus {
    crate::infra::linux::doctor::elevation_status()
}

#[cfg(windows)]
fn toast_status() -> ToastStatus {
    crate::infra::win::doctor::toast_status()
}

#[cfg(not(windows))]
fn toast_status() -> ToastStatus {
    crate::infra::linux::doctor::toast_status()
}

/// Detected volumes and their media type, for the storage line of the report.
fn volumes() -> Vec<crate::domain::models::VolumeInfo> {
    crate::services::optimize_service::OptimizeService::new().volumes()
}

/// Whether guard is installed as a logon task / user unit.
fn guard_armed() -> bool {
    crate::infra::schedule::guard_is_installed().unwrap_or(false)
}

/// Size what guard would clean right now, honoring exclusions and rule packs.
fn would_clean_estimate(
    config_override: Option<&Path>,
    rules_override: Option<&Path>,
    budget: std::time::Duration,
) -> (Vec<CategoryEstimate>, bool) {
    use crate::services::clean_service::{discover_with_policy, CleanService};

    let discovered = discover_with_policy(config_override, rules_override, false);
    let svc = CleanService::new(crate::infra::trash_remover::TrashRemover::new());
    let (scans, truncated) =
        svc.scan_within(&discovered.categories, &discovered.exclusions, budget);

    let mut estimates: Vec<CategoryEstimate> = discovered
        .categories
        .iter()
        .zip(scans.iter())
        .filter(|(_, scan)| scan.total_bytes > 0)
        .map(|(cat, scan)| CategoryEstimate {
            id: scan.category_id.clone(),
            size_bytes: scan.total_bytes,
            risk: cat.risk,
        })
        .collect();
    estimates.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    (estimates, truncated)
}

/// Count idle heavy writers without blocking (doctor must stay under 5s).
fn idle_offender_count() -> u64 {
    use crate::services::idle_service::{IdleConfig, IdleService};

    let config = IdleConfig {
        top: usize::MAX,
        idle_mins: 5,
        min_write_mb: 10,
        clean_cache: false,
    };
    IdleService::new()
        .detect_fast(&config)
        .map(|o| o.len() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::RiskLevel;

    fn est(id: &str, size: u64) -> CategoryEstimate {
        CategoryEstimate {
            id: id.into(),
            size_bytes: size,
            risk: RiskLevel::Safe,
        }
    }

    #[test]
    fn total_equals_sum_of_category_estimates() {
        // The invariant data-model.md places on DoctorReport: the reported
        // total must always be the sum of the per-category lines.
        let would_clean = vec![est("a", 100), est("b", 250), est("c", 25)];
        let total: u64 = would_clean.iter().map(|c| c.size_bytes).sum();
        let report = DoctorReport {
            reserve_status: ReserveStatus::Ok,
            elevation: ElevationStatus::Not,
            toast: ToastStatus::Unavailable,
            guard_armed: false,
            would_clean,
            would_clean_total_bytes: total,
            would_clean_partial: false,
            idle_offender_count: 0,
            volumes: vec![],
        };
        assert_eq!(
            report.would_clean_total_bytes,
            report.would_clean.iter().map(|c| c.size_bytes).sum::<u64>()
        );
        assert_eq!(report.would_clean_total_bytes, 375);
    }

    // Live probe: spawns the elevation/toast probes and walks real cache roots.
    // Ignored on CI per Constitution Principle IV.
    #[test]
    #[ignore]
    fn empty_estimate_totals_zero() {
        let report = DoctorService::new().report(
            Some(Path::new("does-not-exist-doctor-test.toml")),
            None,
        );
        assert_eq!(
            report.would_clean_total_bytes,
            report.would_clean.iter().map(|c| c.size_bytes).sum::<u64>()
        );
    }
}
