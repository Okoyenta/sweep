use std::path::PathBuf;

use crate::domain::models::{CleanCategory, RiskLevel};

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
        cats.push(CleanCategory {
            id: "user-temp".into(),
            title: "User temp files".into(),
            roots: vec![root],
            risk: RiskLevel::Safe,
            cleanup_command: None,
        });
    }

    if let Some(root) = children_root(&lad, &["CrashDumps"]) {
        cats.push(CleanCategory {
            id: "crash-dumps".into(),
            title: "Crash dumps".into(),
            roots: vec![root],
            risk: RiskLevel::Safe,
            cleanup_command: None,
        });
    }

    let browser_caches: Vec<(&str, &str, &[&str])> = vec![
        (
            "chrome-cache",
            "Google Chrome cache",
            &["Google", "Chrome", "User Data", "Default", "Cache"],
        ),
        (
            "chrome-code-cache",
            "Google Chrome code cache",
            &["Google", "Chrome", "User Data", "Default", "Code Cache"],
        ),
        (
            "chrome-gpu",
            "Google Chrome GPU cache",
            &["Google", "Chrome", "User Data", "Default", "GPUCache"],
        ),
        (
            "edge-cache",
            "Microsoft Edge cache",
            &["Microsoft", "Edge", "User Data", "Default", "Cache"],
        ),
        (
            "edge-code-cache",
            "Microsoft Edge code cache",
            &["Microsoft", "Edge", "User Data", "Default", "Code Cache"],
        ),
        (
            "npm-cache",
            "npm package cache",
            &["npm-cache"],
        ),
        (
            "pip-cache",
            "pip package cache",
            &["pip", "cache"],
        ),
    ];

    for (id, title, sub) in browser_caches {
        if let Some(root) = children_root(&lad, &sub) {
            if root.exists() {
                cats.push(CleanCategory {
                    id: id.into(),
                    title: title.into(),
                    roots: vec![root],
                    risk: RiskLevel::Safe,
                    cleanup_command: None,
                });
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
                cats.push(CleanCategory {
                    id: "firefox-cache".into(),
                    title: "Firefox cache".into(),
                    roots,
                    risk: RiskLevel::Safe,
                    cleanup_command: None,
                });
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
                cats.push(CleanCategory {
                    id: "wu-downloads".into(),
                    title: "Windows Update downloads".into(),
                    roots: vec![wu_path],
                    risk: RiskLevel::System,
                    cleanup_command: None,
                });
            }
        }

        if let Some(lad_ref) = local_appdata() {
            let do_path = lad_ref
                .join("Microsoft")
                .join("Windows")
                .join("DeliveryOptimization");
            if do_path.exists() {
                cats.push(CleanCategory {
                    id: "do-cache".into(),
                    title: "Delivery Optimization cache".into(),
                    roots: vec![do_path],
                    risk: RiskLevel::System,
                    cleanup_command: None,
                });
            }
        }

        let driver_store = PathBuf::from("C:\\Windows\\System32\\DriverStore\\FileRepository");
        if driver_store.exists() {
            cats.push(CleanCategory {
                id: "driver-store".into(),
                title: "Driver Store".into(),
                roots: vec![driver_store],
                risk: RiskLevel::System,
                cleanup_command: None,
            });
        }
    }

    cats
}
