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
    safe("edge-gpu", "Microsoft Edge GPU cache"),
    safe("brave-cache", "Brave cache"),
    safe("brave-code-cache", "Brave code cache"),
    safe("brave-gpu", "Brave GPU cache"),
    safe("vivaldi-cache", "Vivaldi cache"),
    safe("vivaldi-code-cache", "Vivaldi code cache"),
    safe("vivaldi-gpu", "Vivaldi GPU cache"),
    safe("opera-cache", "Opera cache"),
    safe("opera-code-cache", "Opera code cache"),
    safe("opera-gpu", "Opera GPU cache"),
    safe("opera-gx-cache", "Opera GX cache"),
    safe("opera-gx-code-cache", "Opera GX code cache"),
    safe("opera-gx-gpu", "Opera GX GPU cache"),
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
    // DISM's component cleanup services WinSxS — the component store under
    // C:\Windows\WinSxS. It is the right command for *this* row and the wrong
    // command for every other row; it used to be printed under Driver Store.
    CategoryDescriptor {
        risk: RiskLevel::System,
        hint: Some("Elevated: dism /Online /Cleanup-Image /StartComponentCleanup"),
        ..safe("winsxs", "WinSxS reclaimable")
    },
    // Report-only: removing package directories from FileRepository corrupts
    // the driver store. Never turn this into a CleanCategory.
    //
    // Reclaiming it is per-package and needs elevation: WinSxS component
    // cleanup does not enumerate or delete third-party driver packages, so it
    // cannot move this row's number at all.
    CategoryDescriptor {
        id: "driver-store",
        title: "Driver Store",
        risk: RiskLevel::System,
        reclaimable: false,
        hint: Some(
            "Elevated: pnputil /enum-drivers, then \
             pnputil /delete-driver <oem##.inf> /uninstall",
        ),
    },
    // `--full` is `clear` plus a re-walk, and `clear` only returns pages to
    // SQLite's freelist — the file keeps them, so a rebuild cannot give space
    // back. `--compact` is the only command that acts on this row's size.
    CategoryDescriptor {
        id: "index",
        title: "Index DB",
        risk: RiskLevel::System,
        reclaimable: false,
        hint: Some("Managed by sweep; 'sweep index --compact' returns its freed pages"),
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

    /// Each hint, the token its own store's tool must contain, and the tokens
    /// that belong to some *other* store.
    ///
    /// The forbidden half is the point. It is what catches a hint printed under
    /// the wrong row — the Driver Store advertising DISM's component cleanup —
    /// which no assertion about the required token alone can see.
    const HINT_TARGETS: &[(&str, &str, &[&str])] = &[
        (
            "winsxs",
            "dism",
            &["pnputil", "pnpm store prune", "sweep index"],
        ),
        (
            "driver-store",
            "pnputil",
            &["dism", "pnpm store prune", "sweep index"],
        ),
        (
            "dev-pnpm",
            "pnpm store prune",
            &["dism", "pnputil", "sweep index"],
        ),
        (
            "index",
            "sweep index --compact",
            &["dism", "pnputil", "pnpm store prune"],
        ),
    ];

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

    /// The ship check for Stage 3: no hint may name a command that acts on some
    /// other category's store.
    ///
    /// The Driver Store row used to advise `dism /Online /Cleanup-Image
    /// /StartComponentCleanup`, which services the WinSxS component store and
    /// cannot touch a driver package. Asserting the required token alone would
    /// not have caught it — the hint *did* name a real command — so each row is
    /// also asserted not to name the other rows' tools.
    #[test]
    fn every_hint_names_a_command_for_its_own_category() {
        for (id, required, forbidden) in HINT_TARGETS {
            let d = lookup(id).unwrap_or_else(|| panic!("'{id}' is not in the registry"));
            let hint = d
                .hint
                .unwrap_or_else(|| panic!("'{id}' is listed as hint-bearing but has no hint"));
            assert!(
                hint.contains(required),
                "{id}: hint must name '{required}'; a category whose hint names no tool \
                 for its own store sends the user nowhere. Got: {hint}"
            );
            for other in *forbidden {
                assert!(
                    !hint.contains(other),
                    "{id}: hint names '{other}', which acts on another store and cannot \
                     affect the row it is printed under. Got: {hint}"
                );
            }
        }
    }

    /// A new hint must arrive with its own-store assertion, or the guard above
    /// silently stops covering the registry as it grows.
    #[test]
    fn every_declared_hint_is_covered_by_the_hint_targets_table() {
        for d in REGISTRY {
            if d.hint.is_some() {
                assert!(
                    HINT_TARGETS.iter().any(|(id, ..)| *id == d.id),
                    "'{}' declares a hint but is not in HINT_TARGETS, so nothing asserts \
                     which store that hint acts on",
                    d.id
                );
            }
        }
    }

    /// The two rows the mis-attached hint confused. Named explicitly because the
    /// generic guard cannot say *which* neighbour a hint belongs to.
    #[test]
    fn driver_store_and_winsxs_hints_name_their_own_stores() {
        let ds = lookup("driver-store").unwrap().hint.unwrap();
        let ws = lookup("winsxs").unwrap().hint.unwrap();
        assert!(ds.contains("pnputil"), "got: {ds}");
        assert!(!ds.contains("dism"), "got: {ds}");
        assert!(ws.contains("dism"), "got: {ws}");
        assert!(!ws.contains("pnputil"), "got: {ws}");
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
