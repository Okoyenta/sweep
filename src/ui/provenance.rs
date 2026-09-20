//! Provenance for every answer derived from the index.
//!
//! `status`, `index --status` and `dupes` all report on the index's contents,
//! and none of them used to say how old those contents were or how much of the
//! disk they covered. A clean negative — `no duplicate groups found` — then
//! reads as a statement about the disk rather than about the index, which is
//! worse than an error: an error tells you to look again, and this does not.
//!
//! The line is a disclosure, not a precision claim. `IndexStats` counts
//! readable file bytes only, so a low coverage figure cannot be attributed to
//! the scope rather than to directories the walk could not open, and the
//! wording deliberately stays on the "covers N of X" side of that ambiguity.

use std::path::PathBuf;

use crate::infra::paths::volumes_total_for;
use crate::services::index_service::IndexProvenance;
use crate::ui::status::fmt;

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// Seconds since the epoch, for callers that only need a "now" to age against.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// An age as a noun phrase: `14 days`, `3 hours`, `less than a minute`.
pub fn age_units(secs: i64) -> String {
    let d = secs.max(0);
    if d < MINUTE {
        "less than a minute".to_string()
    } else if d < HOUR {
        plural(d / MINUTE, "minute")
    } else if d < DAY {
        plural(d / HOUR, "hour")
    } else {
        plural(d / DAY, "day")
    }
}

/// An age as a trailing phrase: `14 days ago`, `3 hours ago`, `just now`.
pub fn age_ago(secs: i64) -> String {
    let d = secs.max(0);
    if d < MINUTE {
        "just now".to_string()
    } else {
        format!("{} ago", age_units(d))
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

/// The absolute-and-relative stamp for a `last run:` field.
///
/// A bare unix timestamp is a number the reader has to decode; this is the same
/// fact in the two forms that are actually useful — when it was, and how long
/// ago that is.
pub fn last_run_line(unix: Option<i64>, now: i64) -> String {
    let Some(ts) = unix else {
        return "never".to_string();
    };
    let ago = age_ago(now - ts);
    match chrono::DateTime::from_timestamp(ts, 0) {
        Some(dt) => format!("{} ({ago})", dt.format("%Y-%m-%d %H:%M UTC")),
        // Out of chrono's representable range: the age is still true, so keep it
        // rather than discarding the whole stamp.
        None => ago,
    }
}

/// One line saying how old the index is and how much of the disk it saw.
pub fn index_provenance(prov: &IndexProvenance, indexed_bytes: u64, now: i64) -> String {
    let Some(last_run) = prov.last_run_unix else {
        return "index: never built — run `sweep index` first".to_string();
    };

    let mut line = format!(
        "index: {} old, covers {}",
        age_units(now - last_run),
        coverage_line(prov, indexed_bytes)
    );
    if prov.interrupted {
        // Deliberately not "run `sweep index` for current results": a rerun is
        // the remedy, but the figures on screen are a lower bound and must not
        // be presented as merely out of date.
        line.push_str(" (last run was interrupted — figures are partial)");
    } else {
        line.push_str(" — run `sweep index` for current results");
    }
    line
}

/// The indexed bytes as a share of what the roots cover:
/// `24.14 GiB of C:\ (11%)`, or `24.14 GiB (scope not recorded)`.
///
/// The denominator is the capacity of the volumes the roots live on, so an index
/// built over one directory of a large disk reports a small share rather than a
/// round 100 %. An index built before the scope was recorded cannot report a
/// share at all, and says which fact is missing instead of implying full cover.
pub fn coverage_line(prov: &IndexProvenance, indexed_bytes: u64) -> String {
    match scope(prov) {
        Some((total, mounts)) if total > 0 => {
            let pct = (indexed_bytes as f64 / total as f64) * 100.0;
            // `{:.0}` would render a real 0.4 % as `0%`, which reads as "none of
            // it" — a different and false claim about an index that did see
            // something. Only an index of literally nothing is 0 %.
            let share = if indexed_bytes > 0 && pct < 0.5 {
                "<1%".to_string()
            } else {
                format!("{pct:.0}%")
            };
            format!(
                "{} of {} ({share})",
                fmt(indexed_bytes),
                mounts_label(&mounts)
            )
        }
        _ => format!("{} (scope not recorded)", fmt(indexed_bytes)),
    }
}

/// Summed capacity of the volumes the recorded roots live on, with the mounts.
fn scope(prov: &IndexProvenance) -> Option<(u64, Vec<PathBuf>)> {
    if prov.roots.is_empty() {
        return None;
    }
    volumes_total_for(&prov.roots)
}

fn mounts_label(mounts: &[PathBuf]) -> String {
    match mounts {
        [one] => one.display().to_string(),
        many => format!("{} volumes", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stamp from the bug report, which printed as `1787438843`.
    const LAST_RUN: i64 = 1_787_438_843;

    fn at_last_run(roots: &[&str], interrupted: bool) -> IndexProvenance {
        IndexProvenance {
            last_run_unix: Some(LAST_RUN),
            roots: roots.iter().map(PathBuf::from).collect(),
            interrupted,
        }
    }

    #[test]
    fn age_units_covers_every_bucket() {
        assert_eq!(age_units(0), "less than a minute");
        assert_eq!(age_units(59), "less than a minute");
        assert_eq!(age_units(60), "1 minute");
        assert_eq!(age_units(120), "2 minutes");
        assert_eq!(age_units(3600), "1 hour");
        assert_eq!(age_units(7200), "2 hours");
        assert_eq!(age_units(86_400), "1 day");
        assert_eq!(age_units(14 * 86_400), "14 days");
        // Clock skew must not produce "less than a minute" for a future stamp on
        // the wrong side of zero.
        assert_eq!(age_units(-500), "less than a minute");
        assert_eq!(age_ago(-500), "just now");
    }

    /// The exact transformation the bug asked for: `1787438843` is a number;
    /// `2026-08-22 22:47 UTC (14 days ago)` is a fact.
    #[test]
    fn last_run_line_is_utc_and_relative() {
        assert_eq!(
            last_run_line(Some(LAST_RUN), LAST_RUN + 14 * 86_400),
            "2026-08-22 22:47 UTC (14 days ago)"
        );
        assert_eq!(
            last_run_line(Some(LAST_RUN), LAST_RUN + 30),
            "2026-08-22 22:47 UTC (just now)"
        );
        assert_eq!(last_run_line(None, LAST_RUN), "never");
    }

    #[test]
    fn provenance_reports_age_and_coverage() {
        // Roots on a real volume, so the denominator resolves. `index_db_path`
        // is on the same volume as the data dir on a default install.
        let roots = vec![crate::infra::paths::data_dir()];
        let prov = IndexProvenance {
            last_run_unix: Some(LAST_RUN),
            roots,
            interrupted: false,
        };
        let line = index_provenance(&prov, 1024, LAST_RUN + 14 * 86_400);
        assert!(
            line.starts_with("index: 14 days old, covers 1.00 KiB of "),
            "got: {line}"
        );
        assert!(
            line.ends_with("— run `sweep index` for current results"),
            "got: {line}"
        );
        assert!(
            line.contains('%'),
            "coverage must be a share, not a bare size: {line}"
        );
    }

    /// An index built before the scope was recorded must not be presented as if
    /// it covered the disk.
    #[test]
    fn provenance_says_scope_not_recorded_without_roots() {
        let line = index_provenance(&at_last_run(&[], false), 4096, LAST_RUN + 86_400);
        assert!(line.contains("scope not recorded"), "got: {line}");
        assert!(!line.contains('%'), "got: {line}");
    }

    /// A run stopped early is a lower bound, which is a different thing from a
    /// run that merely happened a while ago.
    #[test]
    fn provenance_flags_an_interrupted_run() {
        let line = index_provenance(&at_last_run(&[], true), 4096, LAST_RUN + 86_400);
        assert!(line.contains("interrupted"), "got: {line}");
        assert!(line.contains("partial"), "got: {line}");
        assert!(!line.contains("current results"), "got: {line}");
    }

    #[test]
    fn provenance_says_never_built_when_there_is_no_run() {
        let line = index_provenance(&IndexProvenance::default(), 0, LAST_RUN);
        assert_eq!(line, "index: never built — run `sweep index` first");
    }

    /// A small but real coverage must not print as `0%`, which would be a false
    /// claim that the index saw none of the disk.
    #[test]
    fn tiny_but_nonzero_coverage_is_not_reported_as_zero_percent() {
        let roots = vec![crate::infra::paths::data_dir()];
        let prov = IndexProvenance {
            last_run_unix: Some(LAST_RUN),
            roots,
            interrupted: false,
        };
        let line = coverage_line(&prov, 1024);
        assert!(line.contains("<1%"), "got: {line}");
        assert!(!line.contains("(0%)"), "got: {line}");
        // An index of literally nothing is still 0 %.
        assert!(coverage_line(&prov, 0).contains("(0%)"));
    }

    #[test]
    fn mounts_label_counts_volumes() {
        assert_eq!(mounts_label(&[PathBuf::from("C:\\")]), "C:\\");
        assert_eq!(
            mounts_label(&[PathBuf::from("C:\\"), PathBuf::from("D:\\")]),
            "2 volumes"
        );
    }
}
