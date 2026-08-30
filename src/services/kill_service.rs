//! Controlled process termination with a hard system-critical blocklist.
//!
//! This service is the single gate for any process close/kill performed by
//! `sweep idle`, `sweep bg`, or `sweep guard --allow-kill`. It enforces
//! Constitution Principle II: never silently terminate, never touch system
//! processes, and require explicit consent for forced kills.

use crate::domain::models::{KillMode, KillRequest};

/// Hard blocklist of process names that must never be closed or killed.
const BLOCKED_NAMES: &[&str] = &["csrss", "wininit", "services"];

/// Service that validates and executes controlled process termination requests.
pub struct KillService;

impl KillService {
    /// Create a kill service. Holds no state; the blocklist is a constant.
    pub fn new() -> Self {
        Self
    }

    /// True when the request targets a system-critical process or sweep itself.
    pub fn is_blocked(req: &KillRequest) -> bool {
        let self_pid = std::process::id();
        if req.pid == 0 || req.pid == 4 || req.pid == self_pid {
            return true;
        }
        let name = req.name.to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name);
        BLOCKED_NAMES.contains(&name)
    }

    /// Execute a termination request after blocklist + consent checks.
    ///
    /// Returns true if an action was taken. Blocklisted targets and unconsented
    /// forced kills return false without side effects.
    pub fn execute(&self, req: &KillRequest) -> bool {
        if Self::is_blocked(req) {
            return false;
        }
        if req.mode == KillMode::Kill && !req.consent {
            return false;
        }
        match req.mode {
            KillMode::Close => graceful_close(req.pid),
            KillMode::Kill => kill(req.pid),
        }
    }
}

#[cfg(windows)]
fn graceful_close(pid: u32) -> bool {
    crate::infra::win::process_lock::graceful_close(pid)
}

#[cfg(windows)]
fn kill(pid: u32) -> bool {
    crate::infra::win::process_lock::kill(pid)
}

#[cfg(not(windows))]
fn graceful_close(pid: u32) -> bool {
    crate::infra::linux::process_lock::graceful_close(pid)
}

#[cfg(not(windows))]
fn kill(pid: u32) -> bool {
    crate::infra::linux::process_lock::kill(pid)
}

impl Default for KillService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::RiskLevel;

    fn req(pid: u32, name: &str, mode: KillMode, consent: bool) -> KillRequest {
        KillRequest {
            pid,
            name: name.into(),
            size_bytes: 0,
            mode,
            consent,
        }
    }

    #[test]
    fn blocks_system_pids() {
        assert!(KillService::is_blocked(&req(0, "x", KillMode::Kill, true)));
        assert!(KillService::is_blocked(&req(4, "x", KillMode::Kill, true)));
        assert!(KillService::is_blocked(&req(std::process::id(), "sweep", KillMode::Kill, true)));
    }

    #[test]
    fn blocks_system_names_with_and_without_exe() {
        assert!(KillService::is_blocked(&req(100, "csrss", KillMode::Close, false)));
        assert!(KillService::is_blocked(&req(100, "wininit.exe", KillMode::Kill, true)));
        assert!(KillService::is_blocked(&req(100, "SERVICES", KillMode::Kill, true)));
    }

    #[test]
    fn unconsented_kill_is_rejected() {
        let svc = KillService::new();
        let r = req(12345, "notepad", KillMode::Kill, false);
        assert!(!KillService::is_blocked(&r));
        assert!(!svc.execute(&r));
    }
}
