use crate::domain::models::SystemSnapshot;
use crate::domain::traits::SystemMonitor;

pub struct SystemService<M: SystemMonitor> {
    monitor: M,
}

impl<M: SystemMonitor> SystemService<M> {
    pub fn new(monitor: M) -> Self {
        Self { monitor }
    }

    pub fn status_report(&mut self, top_processes: usize) -> anyhow::Result<SystemSnapshot> {
        let mut snap = self.monitor.snapshot()?;
        snap.top_processes.truncate(top_processes);
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{MemoryStats, ProcessMemInfo};

    fn proc(pid: u32, name: &str, mem: u64) -> ProcessMemInfo {
        ProcessMemInfo {
            pid,
            name: name.into(),
            memory_bytes: mem,
            read_bytes: 0,
            write_bytes: 0,
            total_written_bytes: 0,
        }
    }

    struct MockMonitor;

    impl SystemMonitor for MockMonitor {
        fn snapshot(&mut self) -> anyhow::Result<SystemSnapshot> {
            Ok(SystemSnapshot {
                memory: MemoryStats {
                    total_bytes: 16_000_000_000,
                    used_bytes: 8_000_000_000,
                    available_bytes: 8_000_000_000,
                    swap_total_bytes: 0,
                    swap_used_bytes: 0,
                },
                disks: vec![],
                top_processes: vec![
                    proc(42, "chrome", 900),
                    proc(7, "code", 500),
                    proc(11, "mock", 100),
                ],
            })
        }
    }

    #[test]
    fn status_report_returns_monitor_data() {
        let mut svc = SystemService::new(MockMonitor);
        let snap = svc.status_report(10).unwrap();
        assert_eq!(snap.memory.total_bytes, 16_000_000_000);
        assert_eq!(snap.top_processes.len(), 3);
        assert!(snap.disks.is_empty());
    }

    #[test]
    fn status_report_truncates_to_requested_top() {
        let mut svc = SystemService::new(MockMonitor);
        let snap = svc.status_report(2).unwrap();
        assert_eq!(snap.top_processes.len(), 2);
        assert_eq!(snap.top_processes[0].name, "chrome");
        assert_eq!(snap.top_processes[1].name, "code");
    }
}

/// Repository queried for the latest published release.
const RELEASE_API: &str =
    "https://api.github.com/repos/Okoyenta/sweep/releases/latest";

/// Seconds to wait for the update check before giving up.
const UPDATE_TIMEOUT_SECS: u64 = 2;

/// Result of the `sweep --version` update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    /// The running version is the newest published release.
    UpToDate,
    /// A newer release exists; carries its tag.
    Available(String),
    /// The check could not run (offline, timeout, non-2xx). Not an error.
    Skipped,
}

/// Check GitHub Releases for a newer version than `current`.
///
/// Shells out to `curl` with a short timeout rather than pulling in an HTTP
/// stack, matching how sweep already invokes PowerShell for toasts (Principle I:
/// no new heavy dependencies). Any failure — no curl, offline, timeout, bad
/// JSON — yields [`UpdateCheck::Skipped`]; being offline is a degraded mode, not
/// an error (FR-019).
pub fn check_for_update(current: &str) -> UpdateCheck {
    let out = std::process::Command::new("curl")
        .args([
            "-sSL",
            "--max-time",
            &UPDATE_TIMEOUT_SECS.to_string(),
            "-H",
            "User-Agent: sweep-cli",
            "-H",
            "Accept: application/vnd.github+json",
            RELEASE_API,
        ])
        .output();
    let Ok(out) = out else {
        return UpdateCheck::Skipped;
    };
    if !out.status.success() {
        return UpdateCheck::Skipped;
    }
    let body = String::from_utf8_lossy(&out.stdout);
    match parse_latest_tag(&body) {
        Some(tag) => compare_versions(current, &tag),
        None => UpdateCheck::Skipped,
    }
}

/// Extract `tag_name` from a GitHub release payload.
fn parse_latest_tag(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("tag_name")?
        .as_str()
        .map(|s| s.to_string())
}

/// Compare the running version against a release tag (`v1.2.3` or `1.2.3`).
///
/// Unparseable input is treated as "no update" rather than guessing.
fn compare_versions(current: &str, tag: &str) -> UpdateCheck {
    let latest = tag.trim_start_matches('v');
    match (parse_semver(current), parse_semver(latest)) {
        (Some(cur), Some(new)) if new > cur => UpdateCheck::Available(tag.to_string()),
        (Some(_), Some(_)) => UpdateCheck::UpToDate,
        _ => UpdateCheck::Skipped,
    }
}

/// Parse `major.minor.patch` into a comparable tuple.
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn newer_tag_reports_update_available() {
        assert_eq!(
            compare_versions("0.8.0", "v0.9.0"),
            UpdateCheck::Available("v0.9.0".into())
        );
    }

    #[test]
    fn same_version_is_up_to_date() {
        assert_eq!(compare_versions("0.9.0", "v0.9.0"), UpdateCheck::UpToDate);
    }

    #[test]
    fn older_tag_is_up_to_date() {
        assert_eq!(compare_versions("1.0.0", "v0.9.9"), UpdateCheck::UpToDate);
    }

    #[test]
    fn tag_without_v_prefix_is_handled() {
        assert_eq!(
            compare_versions("0.1.0", "0.2.0"),
            UpdateCheck::Available("0.2.0".into())
        );
    }

    #[test]
    fn unparseable_tag_is_skipped() {
        assert_eq!(compare_versions("0.9.0", "nightly"), UpdateCheck::Skipped);
    }

    #[test]
    fn parses_tag_name_from_release_json() {
        let body = r#"{"tag_name":"v1.2.3","name":"release"}"#;
        assert_eq!(parse_latest_tag(body), Some("v1.2.3".to_string()));
    }

    #[test]
    fn malformed_json_yields_no_tag() {
        assert_eq!(parse_latest_tag("not json"), None);
    }
}
