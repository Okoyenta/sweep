use crate::domain::models::InstalledApp;
use crate::domain::traits::AppInventory;
use crate::services::usage_service::UsageMap;

pub struct AppService<I: AppInventory> {
    inventory: I,
}

impl<I: AppInventory> AppService<I> {
    pub fn new(inventory: I) -> Self {
        Self { inventory }
    }

    pub fn list(&self) -> anyhow::Result<Vec<InstalledApp>> {
        self.inventory.installed_apps()
    }

    pub fn attach_usage(&self, apps: &mut [InstalledApp], usage: &UsageMap) {
        for app in apps.iter_mut() {
            if let Some(t) = guess_last_run(app, usage) {
                app.last_run_unix = Some(t);
            }
        }
    }

    pub fn filter_unused_since(
        &self,
        apps: Vec<InstalledApp>,
        days: u64,
        now_unix: i64,
    ) -> Vec<InstalledApp> {
        let cutoff = now_unix - (days as i64) * 86400;
        apps.into_iter()
            .filter(|a| a.last_run_unix.is_none_or(|t| t <= cutoff))
            .collect()
    }

    pub fn find<'a>(&self, apps: &'a [InstalledApp], query: &str) -> Vec<&'a InstalledApp> {
        let q = query.to_lowercase();
        apps.iter()
            .filter(|a| {
                a.name.to_lowercase() == q || a.name.to_lowercase().contains(&q)
            })
            .collect()
    }
}

fn guess_last_run(app: &InstalledApp, usage: &UsageMap) -> Option<i64> {
    name_words(&app.name)
        .filter_map(|word| {
            usage
                .values()
                .filter(|u| u.exe_name.contains(&word))
                .map(|u| u.last_run_unix)
                .max()
        })
        .max()
}

fn name_words(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().all(char::is_alphanumeric) && w.len() >= 3)
        .map(|w| w.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{AppUsage, UsageSource};
    use crate::domain::traits::AppInventory;
    use std::collections::HashMap;

    struct MockInventory {
        apps: Vec<InstalledApp>,
    }

    impl AppInventory for MockInventory {
        fn installed_apps(&self) -> anyhow::Result<Vec<InstalledApp>> {
            Ok(self.apps.clone())
        }
    }

    fn app(name: &str, version: &str, size: u64) -> InstalledApp {
        InstalledApp {
            name: name.to_string(),
            version: version.to_string(),
            publisher: String::new(),
            install_location: None,
            uninstall_command: None,
            size_bytes: Some(size),
            last_run_unix: None,
        }
    }

    fn usage(exe: &str, ts: i64) -> AppUsage {
        AppUsage {
            exe_name: exe.to_string(),
            last_run_unix: ts,
            run_count: 1,
            source: UsageSource::UserAssist,
        }
    }

    fn service(apps: Vec<InstalledApp>) -> AppService<MockInventory> {
        AppService::new(MockInventory { apps })
    }

    #[test]
    fn attaches_usage_via_first_word_heuristic() {
        let mut apps = vec![app("Google Chrome", "120", 500), app("Totally Unknown", "1", 10)];
        let map: UsageMap = HashMap::from([(
            "chrome.exe".to_string(),
            usage("chrome.exe", 1_700_000_000),
        )]);
        service(vec![]).attach_usage(&mut apps, &map);

        assert_eq!(apps[0].last_run_unix, Some(1_700_000_000));
        assert_eq!(apps[1].last_run_unix, None);
    }

    #[test]
    fn unused_filter_keeps_unknown_and_old_only() {
        let now = 2_000_000_000i64;
        let mut recent = app("Recent App", "1", 1);
        recent.last_run_unix = Some(now - 3600);
        let mut old = app("Old App", "1", 1);
        old.last_run_unix = Some(now - 40 * 86400);
        let unknown = app("Mystery App", "1", 1);

        let kept = service(vec![]).filter_unused_since(
            vec![recent, old, unknown],
            30,
            now,
        );
        let names: Vec<&str> = kept.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Old App", "Mystery App"]);
    }

    #[test]
    fn find_matches_case_insensitive_substring() {
        let apps = vec![app("7-Zip 24.0", "24", 1), app("zipper tool", "1", 1)];
        let hits = service(vec![]).find(&apps, "7-zip");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "7-Zip 24.0");
    }
}
