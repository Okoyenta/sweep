use std::path::{Path, PathBuf};

use crate::domain::models::{CleanCategory, RiskLevel};

/// builds cleanable categories under the given cache root; each root's
/// children become removal candidates (see CleanCategory docs)
pub fn build_categories(cache_home: &Path) -> Vec<CleanCategory> {
    let mut cats = Vec::new();

    let browser_caches: Vec<(&str, &str, PathBuf)> = vec![
        (
            "chrome-cache",
            "Google Chrome cache",
            cache_home.join("google-chrome"),
        ),
        ("chromium-cache", "Chromium cache", cache_home.join("chromium")),
        ("edge-cache", "Microsoft Edge cache", cache_home.join("microsoft-edge")),
        ("brave-cache", "Brave cache", cache_home.join("BraveSoftware")),
    ];
    for (id, title, root) in browser_caches {
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

    // firefox: one root per profile's cache2
    let profiles = cache_home.join("mozilla").join("firefox");
    if profiles.exists() {
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

    for (id, title, sub) in [
        ("thumbnails", "Image thumbnails", Some("thumbnails")),
        ("pip-cache", "pip package cache", Some("pip")),
        ("fontconfig", "Fontconfig cache", Some("fontconfig")),
    ] {
        if let Some(sub) = sub {
            let root = cache_home.join(sub);
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

    // npm lives outside ~/.cache by default
    if let Some(home) = std::env::var_os("HOME") {
        let npm_root = PathBuf::from(home).join(".npm").join("_cacache");
        if npm_root.exists() {
            cats.push(CleanCategory {
                id: "npm-cache".into(),
                title: "npm package cache".into(),
                roots: vec![npm_root],
                risk: RiskLevel::Safe,
                cleanup_command: None,
            });
        }
    }

    cats
}

pub fn discover_categories() -> Vec<CleanCategory> {
    let cache_home = match std::env::var_os("XDG_CACHE_HOME") {
        Some(v) => PathBuf::from(v),
        None => match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(".cache"),
            None => return Vec::new(),
        },
    };
    let mut cats = build_categories(&cache_home);
    cats.extend(crate::infra::dev_caches::discover_dev_categories());
    cats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn builds_only_existing_categories() {
        let base = std::env::temp_dir().join(format!("sweep-linux-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("google-chrome")).unwrap();
        fs::create_dir_all(base.join("mozilla/firefox/abc.default/cache2")).unwrap();
        fs::create_dir_all(base.join("pip")).unwrap();

        let cats = build_categories(&base);
        let ids: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"chrome-cache"));
        assert!(ids.contains(&"firefox-cache"));
        assert!(ids.contains(&"pip-cache"));
        assert!(!ids.contains(&"chromium-cache"));

        let ff = cats.iter().find(|c| c.id == "firefox-cache").unwrap();
        assert_eq!(ff.roots.len(), 1);
        assert!(ff.roots[0].ends_with("abc.default/cache2"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn discover_categories_includes_dev_caches() {
        let base = std::env::temp_dir().join(format!(
            "sweep-linux-dev-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".cargo/registry/cache")).unwrap();
        fs::create_dir_all(base.join(".gradle/caches")).unwrap();

        let orig_home = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", base.to_str().unwrap()) };
        let cats = discover_categories();
        let ids: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"dev-cargo"));
        assert!(ids.contains(&"dev-gradle"));

        if let Some(h) = orig_home {
            unsafe { std::env::set_var("HOME", h) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }
        let _ = fs::remove_dir_all(&base);
    }
}
