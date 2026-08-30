//! Windows-specific probes backing `sweep doctor`.
//!
//! Provides elevation and toast-notification availability checks used by the
//! pre-flight report. Both degrade gracefully: a failure to determine state is
//! reported as "not" / "unavailable" rather than aborting the doctor command.

use std::process::Command;

use crate::domain::models::{ElevationStatus, ToastStatus};

/// Whether sweep is running with administrator privileges.
///
/// `net session` succeeds only with elevation, so its exit code is a cheap,
/// dependency-free probe (doctor runs on demand, so the one spawned process is
/// acceptable). Any failure degrades to `Not`.
pub fn elevation_status() -> ElevationStatus {
    let status = Command::new("cmd")
        .args(["/c", "net session >nul 2>&1"])
        .status();
    match status {
        Ok(s) if s.success() => ElevationStatus::Elevated,
        _ => ElevationStatus::Not,
    }
}

/// Whether Windows toast notifications are available to this process.
///
/// Uses a short PowerShell probe for the WinRT toast type; any failure (no
/// PowerShell, server core, headless) yields `Unavailable`.
pub fn toast_status() -> ToastStatus {
    let probe = r#"try { [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; 'ok' } catch { 'no' }"#;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", probe])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.contains("ok") {
                ToastStatus::Available
            } else {
                ToastStatus::Unavailable
            }
        }
        _ => ToastStatus::Unavailable,
    }
}
