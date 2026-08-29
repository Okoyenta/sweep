use std::path::PathBuf;

use crate::domain::models::{CleanCategory, RiskLevel};

pub fn discover_dev_categories() -> Vec<CleanCategory> {
    let mut cats = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();

    #[cfg(windows)]
    {
        if !local.is_empty() {
            let pnpm = PathBuf::from(&local).join("pnpm").join("store");
            if pnpm.exists() {
                cats.push(CleanCategory {
                    id: "dev-pnpm".into(),
                    title: "pnpm store".into(),
                    roots: vec![pnpm],
                    risk: RiskLevel::Safe,
                    cleanup_command: Some("pnpm store prune".into()),
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        if !home.is_empty() {
            let pnpm = PathBuf::from(&home).join(".local/share/pnpm/store");
            if pnpm.exists() {
                cats.push(CleanCategory {
                    id: "dev-pnpm".into(),
                    title: "pnpm store".into(),
                    roots: vec![pnpm],
                    risk: RiskLevel::Safe,
                    cleanup_command: Some("pnpm store prune".into()),
                });
            }
            if let Ok(pnpm_home) = std::env::var("PNPM_HOME") {
                if !pnpm_home.is_empty() {
                    let store = PathBuf::from(&pnpm_home).join("store");
                    if store.exists() && !cats.iter().any(|c| c.id == "dev-pnpm") {
            cats.push(CleanCategory {
                    id: "dev-pnpm".into(),
                    title: "pnpm store".into(),
                    roots: vec![store],
                    risk: RiskLevel::Safe,
                    cleanup_command: Some("pnpm store prune".into()),
                });
                    }
                }
            }
        }
    }

    if !home.is_empty() {
        let cargo_cache = PathBuf::from(&home).join(".cargo/registry/cache");
        let cargo_git = PathBuf::from(&home).join(".cargo/git/checkouts");
        let mut roots = Vec::new();
        if cargo_cache.exists() {
            roots.push(cargo_cache);
        }
        if cargo_git.exists() {
            roots.push(cargo_git);
        }
        if !roots.is_empty() {
            cats.push(CleanCategory {
                id: "dev-cargo".into(),
                title: "cargo cache".into(),
                roots,
                risk: RiskLevel::Safe,
                cleanup_command: None,
            });
        }
    }

    if !home.is_empty() {
        let gradle = PathBuf::from(&home).join(".gradle/caches");
        if gradle.exists() {
            cats.push(CleanCategory {
                id: "dev-gradle".into(),
                title: "gradle caches".into(),
                roots: vec![gradle],
                risk: RiskLevel::Safe,
                cleanup_command: None,
            });
        }
    }

    #[cfg(not(windows))]
    {
        if !home.is_empty() {
            let uv = PathBuf::from(&home).join(".local/share/uv");
            if uv.exists() {
                cats.push(CleanCategory {
                    id: "dev-uv".into(),
                    title: "uv cache".into(),
                    roots: vec![uv],
                    risk: RiskLevel::Safe,
                    cleanup_command: None,
                });
            }
            let pipx = PathBuf::from(&home).join(".local/share/pipx");
            if pipx.exists() {
                cats.push(CleanCategory {
                    id: "dev-pipx".into(),
                    title: "pipx cache".into(),
                    roots: vec![pipx],
                    risk: RiskLevel::Safe,
                    cleanup_command: None,
                });
            }
        }
    }

    cats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn discover_dev_categories_returns_only_existing() {
        let caches = discover_dev_categories();
        for cat in &caches {
            assert!(!cat.id.is_empty());
            assert!(!cat.title.is_empty());
            for root in &cat.roots {
                assert!(root.exists(), "root {} does not exist for category {}", root.display(), cat.id);
            }
        }
    }

    #[test]
    fn discover_dev_categories_skips_missing_roots() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("sweep-dev-cache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("pnpm/store")).unwrap();
        let orig_home = std::env::var("HOME").ok();
        #[cfg(windows)]
        let orig_local = std::env::var("LOCALAPPDATA").ok();
        unsafe { std::env::set_var("HOME", dir.to_str().unwrap()) };
        #[cfg(windows)]
        unsafe { std::env::set_var("LOCALAPPDATA", dir.to_str().unwrap()) };
        let cats = discover_dev_categories();
        let has_pnpm = cats.iter().any(|c| c.id == "dev-pnpm");
        assert!(has_pnpm);
        match orig_home {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        #[cfg(windows)]
        match orig_local {
            Some(v) => unsafe { std::env::set_var("LOCALAPPDATA", v) },
            None => unsafe { std::env::remove_var("LOCALAPPDATA") },
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
