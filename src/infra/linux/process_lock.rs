use std::path::PathBuf;

use crate::domain::models::LockedProcess;

/// Best-effort detection of processes that likely hold a cache/temp path
/// locked, based on matching the running process name against a category's
/// originating application (e.g. a `chrome` process -> the chrome cache
/// category). Linux has no general "which PID holds this file" API, so this
/// is a heuristic and may miss or over-claim; the caller still requires an
/// explicit confirmation before killing.
pub fn locked_processes_for_category(category_id: &str, failed_paths: &[&PathBuf]) -> Vec<LockedProcess> {
    use sysinfo::{ProcessesToUpdate, System};

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let keywords = keywords_for(category_id);
    if keywords.is_empty() {
        return Vec::new();
    }

    let own = std::process::id();
    let mut out = Vec::new();
    for (pid, process) in sys.processes() {
        if *pid.as_u32() == own {
            continue;
        }
        let name = process.name().to_string_lossy().to_lowercase();
        if keywords.iter().any(|k| name.contains(k)) {
            for p in failed_paths.iter() {
                out.push(LockedProcess {
                    pid: *pid.as_u32(),
                    name: process.name().to_string_lossy().into_owned(),
                    path: (*p).clone(),
                });
            }
        }
    }
    out
}

/// maps a category id to the process-name keywords (lowercased) likely to
/// hold its files; empty when the category has no known lock owner
fn keywords_for(category_id: &str) -> Vec<&'static str> {
    match category_id {
        "chrome-cache" | "chrome-code-cache" | "chrome-gpu" => vec!["chrome"],
        "edge-cache" | "edge-code-cache" => vec!["msedge", "edge"],
        "firefox-cache" => vec!["firefox"],
        "npm-cache" => vec!["node", "npm"],
        "pip-cache" => vec!["python", "pip"],
        "dev-pnpm" => vec!["node"],
        _ => Vec::new(),
    }
}

/// Gracefully terminate the process with SIGTERM (`kill -TERM`).
pub fn graceful_close(pid: u32) -> bool {
    let out = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// Forcefully terminate the process with SIGKILL (`kill -KILL`). Blocklist
/// checks are the caller's responsibility (see `kill_service`).
pub fn kill(pid: u32) -> bool {
    let out = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_mapping_covers_known_categories() {
        assert!(keywords_for("chrome-cache").contains(&"chrome"));
        assert!(keywords_for("edge-cache").contains(&"msedge"));
        assert!(keywords_for("firefox-cache").contains(&"firefox"));
        assert!(keywords_for("npm-cache").contains(&"node"));
    }

    #[test]
    fn unknown_category_has_no_keywords() {
        assert!(keywords_for("user-temp").is_empty());
    }
}
