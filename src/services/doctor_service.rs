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
    HEADROOM_THRESHOLD_BYTES, RESERVE_SIZE_BYTES,
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
        // Enumerating disks is cheap but not free, so it joins the thread set
        // rather than adding its cost to the main thread's budget.
        let reserve_probe = std::thread::spawn(reserve_status);

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
        let (reserve_status, reserve_held_bytes, reserve_free_bytes) = reserve_probe
            .join()
            .unwrap_or((ReserveStatus::Missing, 0, 0));

        DoctorReport {
            reserve_status,
            reserve_held_bytes,
            reserve_free_bytes,
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

/// Classify the reserve file from its size, the volume's free space, and whether
/// sweep has run here before.
///
/// Split away from the filesystem reads so every branch is exercised by a table.
/// The below-headroom case only occurs on a disk too full to reproduce on demand,
/// which is exactly the state that went unreported — leaving it untestable would
/// risk it going unreported again.
fn classify_reserve(
    file_len: Option<u64>,
    free_bytes: u64,
    data_dir_exists: bool,
) -> ReserveStatus {
    match file_len {
        Some(len) if len >= RESERVE_SIZE_BYTES => {
            // Size alone says "armed"; free space says whether arming it is still
            // the right call. Below headroom this is space the next write needs.
            if free_bytes < HEADROOM_THRESHOLD_BYTES {
                ReserveStatus::HeldBelowHeadroom
            } else {
                ReserveStatus::Ok
            }
        }
        // Present but short: an allocation that ran out of disk partway, which is
        // not a deliberate release and must not be reported as one.
        Some(_) => ReserveStatus::Partial,
        None => {
            if data_dir_exists {
                ReserveStatus::Consumed
            } else {
                ReserveStatus::Missing
            }
        }
    }
}

/// Read the reserve file and classify it. Returns `(status, bytes held, bytes free)`.
fn reserve_status() -> (ReserveStatus, u64, u64) {
    let free = crate::infra::paths::free_bytes_on_index_volume();
    // A stat failure (missing file, or a permission error) is indistinguishable
    // from absent as far as the report is concerned.
    let len = std::fs::metadata(crate::infra::paths::reserve_path())
        .ok()
        .map(|meta| meta.len());
    let status = classify_reserve(len, free, crate::infra::paths::data_dir().exists());
    (status, len.unwrap_or(0), free)
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
            reserve_held_bytes: 0,
            reserve_free_bytes: 0,
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

    /// Every reserve state, including the one the bug was about.
    ///
    /// The below-headroom row is the regression test: a full-size file on a disk
    /// below `HEADROOM_THRESHOLD_BYTES` used to classify as `Ok` purely on size,
    /// so doctor reported a healthy reserve at the one moment it should have been
    /// released.
    #[test]
    fn reserve_classification_covers_every_state() {
        use crate::domain::models::RECREATION_THRESHOLD_BYTES;

        let full = RESERVE_SIZE_BYTES;
        let roomy = RECREATION_THRESHOLD_BYTES;
        let tight = HEADROOM_THRESHOLD_BYTES - 1;

        assert_eq!(
            classify_reserve(Some(full), roomy, true),
            ReserveStatus::Ok,
            "full reserve with room to spare is armed and harmless"
        );
        assert_eq!(
            classify_reserve(Some(full), tight, true),
            ReserveStatus::HeldBelowHeadroom,
            "full reserve below headroom must not read as ok"
        );
        assert_eq!(
            classify_reserve(Some(full), HEADROOM_THRESHOLD_BYTES, true),
            ReserveStatus::Ok,
            "the headroom boundary itself still counts as roomy"
        );
        assert_eq!(
            classify_reserve(Some(0), roomy, true),
            ReserveStatus::Partial,
            "a zero-length stub is a failed allocation, not a release"
        );
        assert_eq!(
            classify_reserve(Some(full - 1), roomy, true),
            ReserveStatus::Partial,
            "anything under full size is a partial allocation"
        );
        assert_eq!(
            classify_reserve(None, roomy, true),
            ReserveStatus::Consumed,
            "absent on a machine that has a data dir means a previous rescue"
        );
        assert_eq!(
            classify_reserve(None, roomy, false),
            ReserveStatus::Missing,
            "absent with no data dir means sweep never ran here"
        );
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
