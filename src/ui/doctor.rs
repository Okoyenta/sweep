//! Rendering for `sweep doctor`.
//!
//! Prints the stable, script-parseable field set defined in
//! `specs/003-trust-control/contracts/cli.md`. Field names and order are part of
//! the Stage 3 contract; the surrounding prose is not.

use std::io::Write;

use crate::domain::models::{
    DoctorReport, ElevationStatus, ReserveStatus, RiskLevel, ToastStatus,
};
use crate::ui::status::fmt;

/// Print the pre-flight report to stdout.
pub fn print_report(report: &DoctorReport) {
    let mut out = std::io::stdout();
    let _ = write_report(report, &mut out);
}

/// Write the report to `w`, one contract field per line.
pub fn write_report(report: &DoctorReport, w: &mut impl Write) -> std::io::Result<()> {
    writeln!(w, "reserve: {}", reserve_line(report))?;
    writeln!(w, "elevation: {}", elevation_label(report.elevation))?;
    writeln!(w, "toast: {}", toast_label(report.toast))?;
    writeln!(
        w,
        "guard: {}",
        if report.guard_armed {
            "armed"
        } else {
            "not armed"
        }
    )?;
    writeln!(w, "idle: {} offenders", report.idle_offender_count)?;
    for v in &report.volumes {
        writeln!(
            w,
            "storage: {} {} ({})",
            v.mount,
            v.media,
            crate::services::optimize_service::action_for(v.media)
        )?;
    }
    writeln!(
        w,
        "would-clean: {} across {} categories",
        fmt(report.would_clean_total_bytes),
        report.would_clean.len()
    )?;
    if report.would_clean_partial {
        // Say so rather than passing a lower bound off as an exact figure.
        writeln!(
            w,
            "  (partial: sizing budget elapsed; run `sweep clean --scan-only` for exact totals)"
        )?;
    }
    for cat in &report.would_clean {
        writeln!(
            w,
            "  - {}: {} [{}]",
            cat.id,
            fmt(cat.size_bytes),
            risk_label(cat.risk)
        )?;
    }
    if !report.guard_armed {
        writeln!(w, "\nguard is not armed; install it with:")?;
        writeln!(w, "  sweep schedule --guard-install")?;
    }
    if report.elevation == ElevationStatus::Not {
        writeln!(
            w,
            "\nnot elevated: standby purge and service stop are unavailable this run"
        )?;
    }
    Ok(())
}

fn reserve_label(status: ReserveStatus) -> &'static str {
    match status {
        ReserveStatus::Ok => "ok",
        ReserveStatus::Missing => "missing",
        ReserveStatus::Consumed => "consumed",
        // Distinct from `consumed`: that is a reserve spent on purpose, this is one
        // that failed to allocate and must not be mistaken for a working net.
        ReserveStatus::Partial => "partial",
        ReserveStatus::HeldBelowHeadroom => "below-headroom",
    }
}

/// Render the reserve line, including the numbers that make the verdict checkable.
///
/// The status token always occupies the same position so existing parsers keep
/// working; the parenthetical is the only part that varies in shape.
fn reserve_line(report: &DoctorReport) -> String {
    let held = fmt(report.reserve_held_bytes);
    let free = fmt(report.reserve_free_bytes);
    let label = reserve_label(report.reserve_status);
    match report.reserve_status {
        ReserveStatus::Ok => format!("{label} ({held} held, {free} free)"),
        ReserveStatus::HeldBelowHeadroom => format!(
            "{label} ({held} held, {free} free) — below headroom, so this space is needed \
             now; the next `sweep clean` will release it"
        ),
        ReserveStatus::Partial => format!(
            "{label} ({held} held, {free} free) — allocation failed, the net is not armed; \
             retry with `sweep index`"
        ),
        ReserveStatus::Missing => format!(
            "{label} ({free} free) — no reserve yet; `sweep index` or `sweep status` will \
             create one if there is room"
        ),
        ReserveStatus::Consumed => {
            format!("{label} ({free} free) — released by a previous disk-full rescue")
        }
    }
}

fn elevation_label(status: ElevationStatus) -> &'static str {
    match status {
        ElevationStatus::Elevated => "elevated",
        ElevationStatus::Not => "not",
    }
}

fn toast_label(status: ToastStatus) -> &'static str {
    match status {
        ToastStatus::Available => "available",
        ToastStatus::Unavailable => "unavailable",
    }
}

fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Safe => "Safe",
        RiskLevel::System => "System",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{CategoryEstimate, MediaType, VolumeInfo};

    fn report() -> DoctorReport {
        DoctorReport {
            reserve_status: ReserveStatus::Ok,
            reserve_held_bytes: 512 * 1024 * 1024,
            reserve_free_bytes: 8 * 1024 * 1024 * 1024,
            elevation: ElevationStatus::Elevated,
            toast: ToastStatus::Available,
            guard_armed: true,
            would_clean: vec![CategoryEstimate {
                id: "user-temp".into(),
                size_bytes: 1024,
                risk: RiskLevel::Safe,
            }],
            would_clean_total_bytes: 1024,
            would_clean_partial: false,
            idle_offender_count: 2,
            volumes: vec![VolumeInfo {
                mount: "C:\\".into(),
                media: MediaType::Ssd,
            }],
        }
    }

    #[test]
    fn prints_contract_fields_in_order() {
        let mut buf = Vec::new();
        write_report(&report(), &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].starts_with("reserve: ok"));
        assert!(lines[1].starts_with("elevation: elevated"));
        assert!(lines[2].starts_with("toast: available"));
        assert!(lines[3].starts_with("guard: armed"));
        assert!(lines[4].starts_with("idle: 2 offenders"));
        assert!(lines[5].starts_with("storage: C:\\ ssd (trim)"));
        assert!(lines[6].starts_with("would-clean: "));
        assert!(lines[6].contains("across 1 categories"));
        assert!(lines[7].contains("user-temp"));
        assert!(lines[7].contains("[Safe]"));
    }

    #[test]
    fn storage_line_is_emitted_per_volume() {
        let mut r = report();
        r.volumes.push(VolumeInfo {
            mount: "D:\\".into(),
            media: MediaType::Hdd,
        });
        let mut buf = Vec::new();
        write_report(&r, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("storage: C:\\ ssd (trim)"));
        assert!(out.contains("storage: D:\\ hdd (defrag)"));
    }

    #[test]
    fn no_volumes_emits_no_storage_line() {
        let mut r = report();
        r.volumes.clear();
        let mut buf = Vec::new();
        write_report(&r, &mut buf).unwrap();
        assert!(!String::from_utf8(buf).unwrap().contains("storage:"));
    }

    #[test]
    fn unarmed_guard_suggests_install_command() {
        let mut r = report();
        r.guard_armed = false;
        let mut buf = Vec::new();
        write_report(&r, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("guard: not armed"));
        assert!(out.contains("sweep schedule --guard-install"));
    }

    #[test]
    fn healthy_reserve_line_reports_held_and_free() {
        let mut buf = Vec::new();
        write_report(&report(), &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let line = out.lines().next().unwrap();
        assert!(line.starts_with("reserve: ok"), "got: {line}");
        assert!(line.contains("512.00 MiB held"), "got: {line}");
        assert!(line.contains("8.00 GiB free"), "got: {line}");
    }

    /// The bug this pins down: a full-size reserve on a nearly-full disk used to
    /// print a bare `ok`, which read as a healthy posture at the one moment the
    /// reserve should have been released.
    #[test]
    fn reserve_held_below_headroom_is_not_reported_as_ok() {
        let mut r = report();
        r.reserve_status = ReserveStatus::HeldBelowHeadroom;
        r.reserve_free_bytes = 237 * 1024 * 1024;
        let mut buf = Vec::new();
        write_report(&r, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let line = out.lines().next().unwrap();
        assert!(line.starts_with("reserve: below-headroom"), "got: {line}");
        assert!(!line.starts_with("reserve: ok"), "got: {line}");
        assert!(line.contains("237.00 MiB free"), "got: {line}");
    }

    #[test]
    fn partial_reserve_is_distinct_from_consumed() {
        let mut r = report();
        r.reserve_status = ReserveStatus::Partial;
        r.reserve_held_bytes = 0;
        let mut buf = Vec::new();
        write_report(&r, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        let line = out.lines().next().unwrap();
        assert!(line.starts_with("reserve: partial"), "got: {line}");
        assert!(line.contains("allocation failed"), "got: {line}");
    }
}
