//! Rendering for `sweep optimize`.
//!
//! Shows the detected media type per volume and the action it implies before
//! anything runs, so the user can see why sweep chose TRIM or defrag.

use std::io::Write;

use crate::domain::models::{MaintenanceAction, OptimizeOutcome, VolumeInfo};

/// Print the volume table (mount, media, planned action).
pub fn print_volumes(volumes: &[VolumeInfo]) {
    let mut out = std::io::stdout();
    let _ = write_volumes(volumes, &mut out);
}

/// Write the volume table to `w`.
pub fn write_volumes(volumes: &[VolumeInfo], w: &mut impl Write) -> std::io::Result<()> {
    use crate::services::optimize_service::action_for;

    if volumes.is_empty() {
        return writeln!(w, "no fixed volumes detected");
    }
    writeln!(w, "  {:<10} {:<8} {}", "VOLUME", "MEDIA", "ACTION")?;
    for v in volumes {
        writeln!(
            w,
            "  {:<10} {:<8} {}",
            v.mount,
            v.media.to_string(),
            action_for(v.media).to_string()
        )?;
    }
    Ok(())
}

/// Print the result of analyzing or maintaining one volume.
pub fn print_outcome(outcome: &OptimizeOutcome, dry_run: bool) {
    let mut out = std::io::stdout();
    let _ = write_outcome(outcome, dry_run, &mut out);
}

/// Write an outcome to `w`.
pub fn write_outcome(
    outcome: &OptimizeOutcome,
    dry_run: bool,
    w: &mut impl Write,
) -> std::io::Result<()> {
    let verb = if matches!(outcome.action, MaintenanceAction::Unsupported(_)) {
        "skipped"
    } else if !outcome.succeeded {
        "failed"
    } else if outcome.applied {
        "completed"
    } else if dry_run {
        "analyzed"
    } else {
        "no change"
    };
    writeln!(
        w,
        "{} [{}]: {} {}",
        outcome.volume.mount, outcome.volume.media, outcome.action, verb
    )?;
    if !outcome.message.is_empty() {
        for line in outcome.message.lines().filter(|l| !l.trim().is_empty()) {
            writeln!(w, "  {}", line.trim())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::MediaType;

    fn outcome(media: MediaType, action: MaintenanceAction, applied: bool) -> OptimizeOutcome {
        OptimizeOutcome {
            volume: VolumeInfo {
                mount: "C:\\".into(),
                media,
            },
            action,
            succeeded: true,
            applied,
            message: String::new(),
        }
    }

    #[test]
    fn volume_table_shows_media_and_action() {
        let volumes = vec![
            VolumeInfo {
                mount: "C:\\".into(),
                media: MediaType::Ssd,
            },
            VolumeInfo {
                mount: "D:\\".into(),
                media: MediaType::Hdd,
            },
        ];
        let mut buf = Vec::new();
        write_volumes(&volumes, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("C:\\"));
        assert!(out.contains("ssd"));
        assert!(out.contains("trim"));
        assert!(out.contains("D:\\"));
        assert!(out.contains("hdd"));
        assert!(out.contains("defrag"));
    }

    #[test]
    fn applied_outcome_reads_completed() {
        let mut buf = Vec::new();
        write_outcome(
            &outcome(MediaType::Ssd, MaintenanceAction::Trim, true),
            false,
            &mut buf,
        )
        .unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("trim completed"));
    }

    #[test]
    fn dry_run_reads_analyzed() {
        let mut buf = Vec::new();
        write_outcome(
            &outcome(MediaType::Hdd, MaintenanceAction::Defrag, false),
            true,
            &mut buf,
        )
        .unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("defrag analyzed"));
    }

    #[test]
    fn unsupported_reads_skipped() {
        let mut buf = Vec::new();
        let mut skipped = outcome(
            MediaType::Unknown,
            MaintenanceAction::Unsupported("no media".into()),
            false,
        );
        skipped.succeeded = false;
        write_outcome(
            &skipped,
            false,
            &mut buf,
        )
        .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("none skipped"));
        assert!(out.contains("unknown"));
    }

    #[test]
    fn failed_run_reads_failed_not_analyzed() {
        // A tool failure during --analyze must not be reported as a successful
        // analysis.
        let mut failed = outcome(MediaType::Ssd, MaintenanceAction::Trim, false);
        failed.succeeded = false;
        failed.message = "access denied".into();
        let mut buf = Vec::new();
        write_outcome(&failed, true, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("trim failed"));
        assert!(!out.contains("analyzed"));
    }

    #[test]
    fn empty_volume_list_reports_cleanly() {
        let mut buf = Vec::new();
        write_volumes(&[], &mut buf).unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("no fixed volumes"));
    }
}
