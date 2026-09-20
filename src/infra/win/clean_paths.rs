use std::path::{Path, PathBuf};

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

/// Chromium-based browsers: (registry id prefix, `User Data` dir under
/// `%LOCALAPPDATA%`). Each has one folder per profile (`Default`,
/// `Profile 1`, …) holding its own caches.
const CHROMIUM_BROWSERS: &[(&str, &[&str])] = &[
    ("chrome", &["Google", "Chrome", "User Data"]),
    ("edge", &["Microsoft", "Edge", "User Data"]),
    ("brave", &["BraveSoftware", "Brave-Browser", "User Data"]),
    ("vivaldi", &["Vivaldi", "User Data"]),
    ("opera", &["Opera Software", "Opera Stable"]),
    ("opera-gx", &["Opera Software", "Opera GX Stable"]),
];

/// Cache folders inside a Chromium profile, with the id suffix of each.
const CHROMIUM_CACHES: &[(&str, &str)] = &[
    ("cache", "Cache"),
    ("code-cache", "Code Cache"),
    ("gpu", "GPUCache"),
];

/// Every profile folder under a Chromium `User Data` dir, sorted.
///
/// A profile is a folder holding `Preferences` or a `Cache`/`Code Cache`
/// folder. The cache check matters for Opera, which keeps `Preferences` in
/// Roaming but its cache in Local. Browser-wide folders that hold only a
/// `GPUCache` (`ShaderCache`, `GPUPersistentCache`) are not profiles. The
/// `User Data` dir itself counts when it has a cache (older, flat Opera).
fn chromium_profiles(user_data: &Path) -> Vec<PathBuf> {
    let is_profile = |p: &Path| {
        p.join("Preferences").is_file() || p.join("Cache").is_dir() || p.join("Code Cache").is_dir()
    };
    let mut profiles: Vec<PathBuf> = std::fs::read_dir(user_data)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && is_profile(p))
                .collect()
        })
        .unwrap_or_default();
    profiles.sort();
    if user_data.join("Cache").is_dir() || user_data.join("Code Cache").is_dir() {
        profiles.insert(0, user_data.to_path_buf());
    }
    profiles
}

/// One category per browser and cache kind, with one root per profile that
/// has that cache.
fn chromium_categories(lad: &Path) -> Vec<CleanCategory> {
    let mut cats = Vec::new();
    for (browser, sub) in CHROMIUM_BROWSERS {
        let user_data = sub.iter().fold(lad.to_path_buf(), |p, part| p.join(part));
        let profiles = chromium_profiles(&user_data);
        for (suffix, dir) in CHROMIUM_CACHES {
            let roots: Vec<PathBuf> = profiles
                .iter()
                .map(|p| p.join(dir))
                .filter(|p| p.is_dir())
                .collect();
            if !roots.is_empty() {
                cats.push(category(&format!("{browser}-{suffix}"), roots));
            }
        }
    }
    cats
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

    cats.extend(chromium_categories(&lad));

    let tool_caches: Vec<(&str, &[&str])> = vec![
        ("npm-cache", &["npm-cache"]),
        ("pip-cache", &["pip", "cache"]),
    ];

    for (id, sub) in tool_caches {
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

    /// A fake `%LOCALAPPDATA%` for browser discovery tests.
    fn fake_lad(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("sweep-browsers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn mkdirs(base: &Path, rel: &str) {
        std::fs::create_dir_all(base.join(rel)).unwrap();
    }

    fn touch(base: &Path, rel: &str) {
        std::fs::write(base.join(rel), b"{}").unwrap();
    }

    fn roots_of<'a>(cats: &'a [CleanCategory], id: &str) -> Vec<&'a PathBuf> {
        cats.iter()
            .find(|c| c.id == id)
            .map(|c| c.roots.iter().collect())
            .unwrap_or_default()
    }

    /// Regression for browser-profiles-missed: every Chrome profile is
    /// scanned, not just `Default`.
    #[test]
    fn chrome_covers_every_profile() {
        let lad = fake_lad("chrome");
        let ud = "Google/Chrome/User Data";
        for p in ["Default", "Profile 1"] {
            mkdirs(&lad, &format!("{ud}/{p}/Cache/Cache_Data"));
            mkdirs(&lad, &format!("{ud}/{p}/Code Cache"));
            touch(&lad, &format!("{ud}/{p}/Preferences"));
        }
        // browser-wide GPU folders are not profiles
        mkdirs(&lad, &format!("{ud}/ShaderCache/GPUCache"));
        mkdirs(&lad, &format!("{ud}/GPUPersistentCache/GPUCache"));

        let cats = chromium_categories(&lad);
        let user_data = lad.join("Google").join("Chrome").join("User Data");
        assert_eq!(
            roots_of(&cats, "chrome-cache"),
            vec![&user_data.join("Default").join("Cache"), &user_data.join("Profile 1").join("Cache")]
        );
        assert_eq!(roots_of(&cats, "chrome-code-cache").len(), 2);
        // no profile has a GPUCache, and ShaderCache/GPUPersistentCache don't count
        assert!(roots_of(&cats, "chrome-gpu").is_empty());
        let _ = std::fs::remove_dir_all(&lad);
    }

    #[test]
    fn profile_without_a_cache_kind_adds_no_empty_root() {
        let lad = fake_lad("partial");
        let ud = "Microsoft/Edge/User Data";
        mkdirs(&lad, &format!("{ud}/Default/Cache"));
        mkdirs(&lad, &format!("{ud}/Default/GPUCache"));
        mkdirs(&lad, &format!("{ud}/Profile 2/Cache"));
        let cats = chromium_categories(&lad);
        assert_eq!(roots_of(&cats, "edge-cache").len(), 2);
        assert_eq!(roots_of(&cats, "edge-gpu").len(), 1);
        assert!(cats.iter().all(|c| c.id != "edge-code-cache"));
        let _ = std::fs::remove_dir_all(&lad);
    }

    /// Opera keeps `Preferences` in Roaming and only the cache in Local.
    #[test]
    fn opera_split_layout_is_found_without_preferences() {
        let lad = fake_lad("opera");
        mkdirs(&lad, "Opera Software/Opera Stable/Default/Cache/Cache_Data");
        let cats = chromium_categories(&lad);
        assert_eq!(
            roots_of(&cats, "opera-cache"),
            vec![&lad.join("Opera Software").join("Opera Stable").join("Default").join("Cache")]
        );
        let _ = std::fs::remove_dir_all(&lad);
    }

    /// Older Opera kept `Cache` directly in `Opera Stable`.
    #[test]
    fn flat_layout_counts_the_user_data_dir_itself() {
        let lad = fake_lad("flat");
        mkdirs(&lad, "Opera Software/Opera GX Stable/Cache");
        let cats = chromium_categories(&lad);
        assert_eq!(
            roots_of(&cats, "opera-gx-cache"),
            vec![&lad.join("Opera Software").join("Opera GX Stable").join("Cache")]
        );
        let _ = std::fs::remove_dir_all(&lad);
    }

    #[test]
    fn no_browsers_installed_means_no_browser_categories() {
        let lad = fake_lad("none");
        assert!(chromium_categories(&lad).is_empty());
        let _ = std::fs::remove_dir_all(&lad);
    }

    #[test]
    fn every_browser_category_id_is_registered() {
        for (browser, _) in CHROMIUM_BROWSERS {
            for (suffix, _) in CHROMIUM_CACHES {
                let id = format!("{browser}-{suffix}");
                assert!(lookup(&id).is_some(), "{id} is not in the registry");
            }
        }
    }

    /// A path exclusion on one profile drops that profile only, through the
    /// same two steps `discover_with_policy` + `scan_excluding` use.
    #[test]
    fn excluding_one_profile_keeps_the_others() {
        use crate::domain::models::ExclusionConfig;
        use crate::services::clean_service::CleanService;
        use crate::services::exclusion_service::apply_exclusions;

        let lad = fake_lad("excl");
        let ud = "Google/Chrome/User Data";
        for p in ["Default", "Profile 1"] {
            mkdirs(&lad, &format!("{ud}/{p}/Cache/Cache_Data"));
            std::fs::write(lad.join(format!("{ud}/{p}/Cache/Cache_Data/f_000001")), vec![0u8; 10])
                .unwrap();
        }
        let cats: Vec<_> = chromium_categories(&lad)
            .into_iter()
            .filter(|c| c.id == "chrome-cache")
            .collect();
        let excl = ExclusionConfig {
            paths: vec![lad.join("Google").join("Chrome").join("User Data").join("Profile 1")],
            ..Default::default()
        };
        let (cats, excluded) = apply_exclusions(&cats, &excl);
        assert_eq!(excluded, 0, "one excluded profile must not hide the category");
        let svc = CleanService::new(crate::infra::trash_remover::TrashRemover::new());
        let scans = svc.scan_excluding(&cats, &excl);
        assert_eq!(scans[0].items.len(), 1, "{:?}", scans[0].items);
        assert!(scans[0].items[0].starts_with(lad.join("Google").join("Chrome").join("User Data").join("Default")));
        assert_eq!(scans[0].total_bytes, 10);
        let _ = std::fs::remove_dir_all(&lad);
    }
}
