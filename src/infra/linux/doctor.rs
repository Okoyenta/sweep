//! Linux-specific probes backing `sweep doctor`.
//!
//! Elevation is determined via the effective UID read from `/proc/self/status`
//! (no extra crate needed); toast notifications are not available on Linux
//! (headless/server environments), so the probe always reports `Unavailable`.

use crate::domain::models::{ElevationStatus, ToastStatus};

/// Whether sweep is running as root (effective UID 0).
pub fn elevation_status() -> ElevationStatus {
    let status = match std::fs::read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(_) => return ElevationStatus::Not,
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            // Fields: real effective saved fs (whitespace-separated).
            if let Some(effective) = rest.split_whitespace().nth(1) {
                if effective == "0" {
                    return ElevationStatus::Elevated;
                }
            }
        }
    }
    ElevationStatus::Not
}

/// Toast notifications are not supported on Linux; always `Unavailable`.
pub fn toast_status() -> ToastStatus {
    ToastStatus::Unavailable
}
