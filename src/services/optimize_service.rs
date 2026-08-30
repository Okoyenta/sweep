//! Drive maintenance: TRIM for solid-state volumes, defrag for rotational ones.
//!
//! Sweep frees space; this service keeps the drive healthy afterwards. The
//! action is chosen from the detected media type and never guessed: an
//! undetectable drive gets no maintenance at all.
//!
//! The core invariant is [`action_for`]: a solid-state volume is never
//! defragmented. Defrag rewrites large amounts of data to reorder blocks, which
//! on flash burns write cycles for no seek benefit — the same class of
//! "never do this to the user's hardware" rule as the kill blocklist in
//! `kill_service` (Constitution Principle II).

use crate::domain::models::{MaintenanceAction, MediaType, OptimizeOutcome, VolumeInfo};

/// Service that plans and runs volume maintenance.
pub struct OptimizeService;

impl OptimizeService {
    /// Create an optimize service. Holds no state; media is probed per call.
    pub fn new() -> Self {
        Self
    }

    /// List the volumes sweep can maintain, with their detected media type.
    pub fn volumes(&self) -> Vec<VolumeInfo> {
        volumes()
    }

    /// Analyze or maintain one volume.
    ///
    /// With `dry_run` set, nothing is modified: Windows runs
    /// `Optimize-Volume -Analyze` and Linux runs `fstrim --dry-run`, so the user
    /// can see what would happen before consenting.
    pub fn run(&self, volume: &VolumeInfo, dry_run: bool) -> OptimizeOutcome {
        let action = action_for(volume.media);
        if let MaintenanceAction::Unsupported(reason) = &action {
            return OptimizeOutcome {
                volume: volume.clone(),
                action: action.clone(),
                succeeded: false,
                applied: false,
                message: reason.clone(),
            };
        }
        match execute(volume, &action, dry_run) {
            Ok(message) => OptimizeOutcome {
                volume: volume.clone(),
                action,
                succeeded: true,
                applied: !dry_run,
                message,
            },
            Err(e) => OptimizeOutcome {
                volume: volume.clone(),
                action,
                succeeded: false,
                applied: false,
                message: summarize_error(&format!("{e}")),
            },
        }
    }
}

impl Default for OptimizeService {
    fn default() -> Self {
        Self::new()
    }
}

/// Choose the maintenance action for a media type.
///
/// This is the safety gate for the whole feature:
/// - solid-state -> TRIM, never defrag (defrag would burn write cycles)
/// - rotational -> defrag, which is where fragmentation actually costs seeks
/// - unknown -> nothing, because acting on a guess could mean defragging flash
pub fn action_for(media: MediaType) -> MaintenanceAction {
    match media {
        MediaType::Ssd => MaintenanceAction::Trim,
        MediaType::Hdd => MaintenanceAction::Defrag,
        MediaType::Unknown => MaintenanceAction::Unsupported(
            "media type could not be determined; refusing to guess (defragmenting an SSD would \
             wear it for no benefit)"
                .to_string(),
        ),
    }
}

/// Reduce a tool failure to one actionable line.
///
/// `Optimize-Volume` and `fstrim` both fail with a multi-line stack trace when
/// the user is not elevated; that is by far the most common failure, so it is
/// translated into a single instruction instead of dumped verbatim.
pub fn summarize_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("access denied")
        || lower.contains("access is denied")
        || lower.contains("permission denied")
        || lower.contains("operation not permitted")
    {
        return if cfg!(windows) {
            "access denied — drive maintenance requires an elevated prompt (run as Administrator)"
                .to_string()
        } else {
            "permission denied — fstrim requires root (try sudo)".to_string()
        };
    }
    // Otherwise keep the first meaningful line; the rest is usually a trace.
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(raw)
        .to_string()
}

/// Human-readable description of what an action will do, for the consent prompt.
pub fn action_description(action: &MaintenanceAction, mount: &str) -> String {
    match action {
        MaintenanceAction::Trim => {
            format!("re-issue TRIM for unused blocks on {mount} (solid-state)")
        }
        MaintenanceAction::Defrag => {
            format!("defragment {mount} (rotational) — this can take a long time")
        }
        MaintenanceAction::Unsupported(reason) => reason.clone(),
    }
}

#[cfg(windows)]
fn volumes() -> Vec<VolumeInfo> {
    crate::infra::win::storage::volumes()
}

#[cfg(not(windows))]
fn volumes() -> Vec<VolumeInfo> {
    crate::infra::linux::storage::volumes()
}

#[cfg(windows)]
fn execute(
    volume: &VolumeInfo,
    action: &MaintenanceAction,
    dry_run: bool,
) -> anyhow::Result<String> {
    use crate::infra::win::storage;

    let Some(letter) = storage::drive_letter(&volume.mount) else {
        anyhow::bail!("{} is not a drive-letter volume", volume.mount);
    };
    let flag = if dry_run {
        "Analyze"
    } else {
        match action {
            MaintenanceAction::Trim => "ReTrim",
            MaintenanceAction::Defrag => "Defrag",
            MaintenanceAction::Unsupported(r) => anyhow::bail!("{r}"),
        }
    };
    storage::optimize_volume(letter, flag)
}

#[cfg(not(windows))]
fn execute(
    volume: &VolumeInfo,
    action: &MaintenanceAction,
    dry_run: bool,
) -> anyhow::Result<String> {
    use crate::infra::linux::storage;

    match action {
        MaintenanceAction::Trim => {
            storage::fstrim(std::path::Path::new(&volume.mount), dry_run)
        }
        // No general-purpose online defragmenter exists on Linux; e4defrag is
        // ext4-only and inappropriate to run unattended.
        MaintenanceAction::Defrag => anyhow::bail!(
            "defragmentation is not supported on Linux; modern filesystems (ext4, xfs, btrfs) \
             manage extents themselves"
        ),
        MaintenanceAction::Unsupported(r) => anyhow::bail!("{r}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssd_is_trimmed_never_defragmented() {
        // The central safety invariant of this service.
        let action = action_for(MediaType::Ssd);
        assert_eq!(action, MaintenanceAction::Trim);
        assert_ne!(action, MaintenanceAction::Defrag);
    }

    #[test]
    fn hdd_is_defragmented() {
        assert_eq!(action_for(MediaType::Hdd), MaintenanceAction::Defrag);
    }

    #[test]
    fn unknown_media_gets_no_action() {
        match action_for(MediaType::Unknown) {
            MaintenanceAction::Unsupported(reason) => {
                assert!(reason.contains("could not be determined"));
            }
            other => panic!("unknown media must not act, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_volume_reports_without_running() {
        let volume = VolumeInfo {
            mount: "Z:\\".into(),
            media: MediaType::Unknown,
        };
        let outcome = OptimizeService::new().run(&volume, false);
        assert!(!outcome.applied);
        assert!(!outcome.succeeded);
        assert!(matches!(
            outcome.action,
            MaintenanceAction::Unsupported(_)
        ));
    }

    #[test]
    fn permission_errors_become_one_actionable_line() {
        let raw = "Optimize-Volume failed: Optimize-Volume : Access denied\nActivity ID: {…}\n\
                   At line:1 char:1\n+ Optimize-Volume -DriveLetter C";
        let summary = summarize_error(raw);
        assert_eq!(summary.lines().count(), 1);
        assert!(summary.contains("elevated") || summary.contains("root"));
        assert!(!summary.contains("At line:1"));
    }

    #[test]
    fn other_errors_keep_their_first_line() {
        let summary = summarize_error("fstrim failed: the discard operation is not supported\ntrace");
        assert_eq!(summary, "fstrim failed: the discard operation is not supported");
    }

    #[test]
    fn descriptions_name_the_volume_and_media() {
        let trim = action_description(&MaintenanceAction::Trim, "C:\\");
        assert!(trim.contains("TRIM"));
        assert!(trim.contains("C:\\"));
        let defrag = action_description(&MaintenanceAction::Defrag, "D:\\");
        assert!(defrag.contains("defragment"));
        assert!(defrag.contains("D:\\"));
    }

    #[test]
    fn action_display_is_stable() {
        assert_eq!(MaintenanceAction::Trim.to_string(), "trim");
        assert_eq!(MaintenanceAction::Defrag.to_string(), "defrag");
        assert_eq!(
            MaintenanceAction::Unsupported("x".into()).to_string(),
            "none"
        );
        assert_eq!(MediaType::Ssd.to_string(), "ssd");
        assert_eq!(MediaType::Hdd.to_string(), "hdd");
        assert_eq!(MediaType::Unknown.to_string(), "unknown");
    }
}
