use std::path::PathBuf;

use crate::domain::categories::category;
use crate::domain::models::CleanCategory;
use crate::infra::paths::home_dir;

/// pnpm's store is hardlinked, so trashing frees nothing; prune it instead.
fn pnpm_category(root: PathBuf) -> CleanCategory {
    CleanCategory {
        cleanup_command: Some("pnpm store prune".into()),
        ..category("dev-pnpm", vec![root])
    }
}

pub fn discover_dev_categories() -> Vec<CleanCategory> {
    let mut cats = Vec::new();

    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        if !local.is_empty() {
            let pnpm = PathBuf::from(&local).join("pnpm").join("store");
            if pnpm.exists() {
                cats.push(pnpm_category(pnpm));
            }
        }
    }

    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            let pnpm = PathBuf::from(&home).join(".local/share/pnpm/store");
            if pnpm.exists() {
                cats.push(pnpm_category(pnpm));
            }
            if let Ok(pnpm_home) = std::env::var("PNPM_HOME") {
                if !pnpm_home.is_empty() {
                    let store = PathBuf::from(&pnpm_home).join("store");
                    if store.exists() && !cats.iter().any(|c| c.id == "dev-pnpm") {
                        cats.push(pnpm_category(store));
                    }
                }
            }
            let uv = PathBuf::from(&home).join(".local/share/uv");
            if uv.exists() {
                cats.push(category("dev-uv", vec![uv]));
            }
            let pipx = PathBuf::from(&home).join(".local/share/pipx");
            if pipx.exists() {
                cats.push(category("dev-pipx", vec![pipx]));
            }
        }
    }

    // cargo and gradle resolve their cache root the way the tool itself does: a
    // relocation env var when set, otherwise the user's home directory. On
    // Windows the home directory is USERPROFILE (a plain PowerShell / cmd /
    // scheduled-task launch never sets HOME), so these caches are found wherever
    // sweep was launched.
    let cargo_home = std::env::var("CARGO_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".cargo")));
    if let Some(cargo_home) = cargo_home {
        let cargo_cache = cargo_home.join("registry/cache");
        let cargo_git = cargo_home.join("git/checkouts");
        let mut roots = Vec::new();
        if cargo_cache.exists() {
            roots.push(cargo_cache);
        }
        if cargo_git.exists() {
            roots.push(cargo_git);
        }
        if !roots.is_empty() {
            cats.push(category("dev-cargo", roots));
        }
    }

    let gradle_home = std::env::var("GRADLE_USER_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".gradle")));
    if let Some(gradle_home) = gradle_home {
        let gradle = gradle_home.join("caches");
        if gradle.exists() {
            cats.push(category("dev-gradle", vec![gradle]));
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

    /// Saves the current value of each named environment variable and restores
    /// it on drop, so a test that mutates process-global state never leaks it
    /// (even when an assertion panics before the manual cleanup runs). Hold
    /// `ENV_LOCK` for as long as an `EnvGuard` is alive.
    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn new(vars: &[&'static str]) -> EnvGuard {
            let saved = vars.iter().map(|&v| (v, std::env::var(v).ok())).collect();
            EnvGuard { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(v) => unsafe { std::env::set_var(key, v) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sweep-dev-cache-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_dev_categories_returns_only_existing() {
        // Serialize with the env-mutating tests below so this reads a stable,
        // unmutated environment.
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
        // Isolate discovery from the real profile so missing roots stay missing.
        // On Windows the profile is USERPROFILE (HOME is ignored there), which the
        // old version of this test hid by setting HOME instead.
        let _env = EnvGuard::new(&["HOME", "USERPROFILE", "LOCALAPPDATA", "CARGO_HOME", "GRADLE_USER_HOME"]);
        let dir = temp_dir("missing");
        #[cfg(windows)]
        fs::create_dir_all(dir.join("pnpm/store")).unwrap();
        #[cfg(not(windows))]
        fs::create_dir_all(dir.join(".local/share/pnpm/store")).unwrap();

        unsafe {
            std::env::set_var("HOME", dir.to_str().unwrap());
            std::env::remove_var("CARGO_HOME");
            std::env::remove_var("GRADLE_USER_HOME");
        }
        #[cfg(windows)]
        unsafe {
            std::env::set_var("LOCALAPPDATA", dir.to_str().unwrap());
            std::env::set_var("USERPROFILE", dir.to_str().unwrap());
            std::env::remove_var("HOME");
        }

        let cats = discover_dev_categories();
        assert!(cats.iter().any(|c| c.id == "dev-pnpm"));
        assert!(
            cats.iter().all(|c| c.id != "dev-cargo" && c.id != "dev-gradle"),
            "missing cargo/gradle roots must not produce categories: {cats:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Regression for missing-dev-caches: with HOME unset, discovery must still
    /// find cargo and gradle through USERPROFILE.
    #[cfg(windows)]
    #[test]
    fn discovers_dev_caches_from_userprofile_when_home_is_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::new(&["HOME", "USERPROFILE", "CARGO_HOME", "GRADLE_USER_HOME"]);
        let dir = temp_dir("home");
        fs::create_dir_all(dir.join(".cargo/registry/cache")).unwrap();
        fs::create_dir_all(dir.join(".gradle/caches")).unwrap();

        // Simulate a plain PowerShell / scheduled-task launch: no HOME, no
        // relocation vars, only USERPROFILE.
        unsafe {
            std::env::remove_var("HOME");
            std::env::remove_var("CARGO_HOME");
            std::env::remove_var("GRADLE_USER_HOME");
            std::env::set_var("USERPROFILE", dir.to_str().unwrap());
        }

        let cats = discover_dev_categories();
        assert!(cats.iter().any(|c| c.id == "dev-cargo"), "dev-cargo missing: {cats:?}");
        assert!(cats.iter().any(|c| c.id == "dev-gradle"), "dev-gradle missing: {cats:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn relocation_env_vars_win_over_the_profile_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::new(&["HOME", "USERPROFILE", "CARGO_HOME", "GRADLE_USER_HOME"]);
        let dir = temp_dir("reloc");
        let cargo_reloc = dir.join("cargo-reloc");
        let gradle_reloc = dir.join("gradle-reloc");
        fs::create_dir_all(cargo_reloc.join("registry/cache")).unwrap();
        fs::create_dir_all(gradle_reloc.join("caches")).unwrap();
        // A home dir with no caches proves the relocation vars win.
        let empty_home = dir.join("empty-home");
        fs::create_dir_all(&empty_home).unwrap();

        unsafe {
            std::env::set_var("CARGO_HOME", cargo_reloc.to_str().unwrap());
            std::env::set_var("GRADLE_USER_HOME", gradle_reloc.to_str().unwrap());
            std::env::set_var("HOME", empty_home.to_str().unwrap());
        }
        #[cfg(windows)]
        unsafe { std::env::set_var("USERPROFILE", empty_home.to_str().unwrap()) };

        let cats = discover_dev_categories();
        let cargo = cats.iter().find(|c| c.id == "dev-cargo").expect("dev-cargo present");
        assert_eq!(cargo.roots, vec![cargo_reloc.join("registry/cache")]);
        let gradle = cats.iter().find(|c| c.id == "dev-gradle").expect("dev-gradle present");
        assert_eq!(gradle.roots, vec![gradle_reloc.join("caches")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_relocation_env_var_falls_back_to_the_profile() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::new(&["HOME", "USERPROFILE", "CARGO_HOME", "GRADLE_USER_HOME"]);
        let dir = temp_dir("empty-reloc");
        fs::create_dir_all(dir.join(".cargo/registry/cache")).unwrap();

        unsafe {
            std::env::set_var("CARGO_HOME", "");
            std::env::remove_var("GRADLE_USER_HOME");
            std::env::set_var("HOME", dir.to_str().unwrap());
        }
        #[cfg(windows)]
        unsafe { std::env::set_var("USERPROFILE", dir.to_str().unwrap()) };

        let cats = discover_dev_categories();
        let cargo = cats.iter().find(|c| c.id == "dev-cargo").expect("dev-cargo present");
        assert_eq!(cargo.roots, vec![dir.join(".cargo/registry/cache")]);
        let _ = fs::remove_dir_all(&dir);
    }
}
