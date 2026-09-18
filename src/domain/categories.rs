//! The single registry of built-in category metadata.
//!
//! `sweep clean` (via `infra::*::clean_paths` / `infra::dev_caches`) and
//! `sweep diagnose` both read id, title, risk and reclaimability from here, so
//! the two commands cannot give opposite verdicts for the same directory.
//! Discovery code only supplies the roots it found on this machine.

use std::path::PathBuf;

use crate::domain::models::{CleanCategory, RiskLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub risk: RiskLevel,
    /// false = sweep may measure and report this category but must never
    /// remove anything in it
    pub reclaimable: bool,
    /// guidance shown by diagnose (e.g. the correct tool to reclaim it)
    pub hint: Option<&'static str>,
}

const fn safe(id: &'static str, title: &'static str) -> CategoryDescriptor {
    CategoryDescriptor {
        id,
        title,
        risk: RiskLevel::Safe,
        reclaimable: true,
        hint: None,
    }
}

pub const REGISTRY: &[CategoryDescriptor] = &[
    // user caches (Windows + Linux)
    safe("user-temp", "User temp files"),
    safe("crash-dumps", "Crash dumps"),
    safe("chrome-cache", "Google Chrome cache"),
    safe("chrome-code-cache", "Google Chrome code cache"),
    safe("chrome-gpu", "Google Chrome GPU cache"),
    safe("chromium-cache", "Chromium cache"),
    safe("edge-cache", "Microsoft Edge cache"),
    safe("edge-code-cache", "Microsoft Edge code cache"),
    safe("brave-cache", "Brave cache"),
    safe("firefox-cache", "Firefox cache"),
    safe("npm-cache", "npm package cache"),
    safe("pip-cache", "pip package cache"),
    safe("thumbnails", "Image thumbnails"),
    safe("fontconfig", "Fontconfig cache"),
    // developer caches
    CategoryDescriptor {
        hint: Some("pnpm uses hardlinks; sweep runs 'pnpm store prune' for safe reclaim"),
        ..safe("dev-pnpm", "pnpm store")
    },
    safe("dev-cargo", "cargo cache"),
    safe("dev-gradle", "gradle caches"),
    safe("dev-uv", "uv cache"),
    safe("dev-pipx", "pipx cache"),
    // deep (system) categories
    CategoryDescriptor {
        risk: RiskLevel::System,
        ..safe("wu-downloads", "Windows Update downloads")
    },
    CategoryDescriptor {
        risk: RiskLevel::System,
        ..safe("do-cache", "Delivery Optimization cache")
    },
    CategoryDescriptor {
        risk: RiskLevel::System,
        ..safe("winsxs", "WinSxS reclaimable")
    },
    // Report-only: removing package directories from FileRepository corrupts
    // the driver store. Never turn this into a CleanCategory.
    CategoryDescriptor {
        id: "driver-store",
        title: "Driver Store",
        risk: RiskLevel::System,
        reclaimable: false,
        hint: Some("Elevated: dism /Online /Cleanup-Image /StartComponentCleanup"),
    },
    CategoryDescriptor {
        id: "index",
        title: "Index DB",
        risk: RiskLevel::System,
        reclaimable: false,
        hint: Some("Managed by sweep; use 'sweep index --full' to rebuild"),
    },
];

pub fn lookup(id: &str) -> Option<&'static CategoryDescriptor> {
    REGISTRY.iter().find(|d| d.id == id)
}

/// Build a built-in [`CleanCategory`] from its registry entry.
///
/// Panics on an unknown id: every built-in category must be registered, and
/// the tests exercise every call site.
pub fn category(id: &str, roots: Vec<PathBuf>) -> CleanCategory {
    let d = lookup(id).unwrap_or_else(|| panic!("category '{id}' is not in the registry"));
    CleanCategory {
        id: d.id.into(),
        title: d.title.into(),
        roots,
        risk: d.risk,
        cleanup_command: None,
        reclaimable: d.reclaimable,
    }
}

/// The Driver Store package repository, resolved from `%SystemRoot%`.
pub fn driver_store_path() -> PathBuf {
    system_root()
        .join("System32")
        .join("DriverStore")
        .join("FileRepository")
}

/// `%SystemRoot%`, falling back to `C:\Windows` when the variable is unset.
pub fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique() {
        for (i, d) in REGISTRY.iter().enumerate() {
            assert!(
                REGISTRY[i + 1..].iter().all(|o| o.id != d.id),
                "duplicate registry id {}",
                d.id
            );
        }
    }

    #[test]
    fn driver_store_is_report_only() {
        let d = lookup("driver-store").unwrap();
        assert!(!d.reclaimable);
        assert_eq!(d.risk, RiskLevel::System);
    }

    #[test]
    fn category_copies_registry_metadata() {
        let c = category("driver-store", vec![]);
        assert_eq!(c.title, "Driver Store");
        assert!(!c.reclaimable);
        assert!(category("user-temp", vec![]).reclaimable);
    }

    #[test]
    #[should_panic(expected = "not in the registry")]
    fn category_rejects_unknown_id() {
        let _ = category("no-such-category", vec![]);
    }
}
