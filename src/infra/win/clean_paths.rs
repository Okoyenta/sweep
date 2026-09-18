use std::path::PathBuf;

use crate::domain::categories::category;
use crate::domain::models::CleanCategory;

fn local_appdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

fn children_root(base: &PathBuf, sub: &[&str]) -> Option<PathBuf> {
    let mut p = base.clone();
    for part in sub {
        p.push(part);
    }
    Some(p)
}

pub fn discover_categories() -> Vec<CleanCategory> {
    discover_categories_inner(false)
}

pub fn discover_categories_deep() -> Vec<CleanCategory> {
    discover_categories_inner(true)
}

fn discover_categories_inner(deep: bool) -> Vec<CleanCategory> {
    let Some(lad) = local_appdata() else {
        return Vec::new();
    };

    let mut cats = Vec::new();

    cats.extend(crate::infra::dev_caches::discover_dev_categories());

    if let Some(root) = children_root(&lad, &["Temp"]) {
        cats.push(category("user-temp", vec![root]));
    }

    if let Some(root) = children_root(&lad, &["CrashDumps"]) {
        cats.push(category("crash-dumps", vec![root]));
    }

    let browser_caches: Vec<(&str, &[&str])> = vec![
        ("chrome-cache", &["Google", "Chrome", "User Data", "Default", "Cache"]),
        ("chrome-code-cache", &["Google", "Chrome", "User Data", "Default", "Code Cache"]),
        ("chrome-gpu", &["Google", "Chrome", "User Data", "Default", "GPUCache"]),
        ("edge-cache", &["Microsoft", "Edge", "User Data", "Default", "Cache"]),
        ("edge-code-cache", &["Microsoft", "Edge", "User Data", "Default", "Code Cache"]),
        ("npm-cache", &["npm-cache"]),
        ("pip-cache", &["pip", "cache"]),
    ];

    for (id, sub) in browser_caches {
        if let Some(root) = children_root(&lad, &sub) {
            if root.exists() {
                cats.push(category(id, vec![root]));
            }
        }
    }

    // firefox: one root per profile's cache2
    if let Some(profiles) = children_root(&lad, &["Mozilla", "Firefox", "Profiles"]) {
        if let Ok(rd) = std::fs::read_dir(&profiles) {
            let roots: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path().join("cache2"))
                .filter(|p| p.exists())
                .collect();
            if !roots.is_empty() {
                cats.push(category("firefox-cache", roots));
            }
        }
    }

    if deep {
        if let Some(program_data) = std::env::var_os("ProgramData") {
            let wu_path = PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("SoftwareDistribution")
                .join("Download");
            if wu_path.exists() {
                cats.push(category("wu-downloads", vec![wu_path]));
            }
        }

        if let Some(lad_ref) = local_appdata() {
            let do_path = lad_ref
                .join("Microsoft")
                .join("Windows")
                .join("DeliveryOptimization");
            if do_path.exists() {
                cats.push(category("do-cache", vec![do_path]));
            }
        }

        // The Driver Store is deliberately absent: it is report-only
        // (`reclaimable: false` in `domain::categories`) and only diagnose
        // measures it. Removing package directories corrupts it.
    }

    cats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::categories::{lookup, system_root};

    #[test]
    fn deep_discovery_never_offers_report_only_or_system32_categories() {
        let system32 = system_root().join("System32").to_string_lossy().to_lowercase();
        for cat in discover_categories_deep() {
            let entry = lookup(&cat.id)
                .unwrap_or_else(|| panic!("category '{}' is not in the registry", cat.id));
            assert_eq!(cat.reclaimable, entry.reclaimable, "{}", cat.id);
            assert!(cat.reclaimable, "clean discovered report-only category '{}'", cat.id);
            assert_ne!(cat.id, "driver-store");
            for root in &cat.roots {
                assert!(
                    !root.to_string_lossy().to_lowercase().starts_with(&system32),
                    "{} root {} is under System32",
                    cat.id,
                    root.display()
                );
            }
        }
    }
}
