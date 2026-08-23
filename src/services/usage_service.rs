use std::collections::HashMap;

use crate::domain::models::AppUsage;
use crate::domain::traits::UsageProbe;

pub type UsageMap = HashMap<String, AppUsage>;

pub struct UsageService {
    probes: Vec<Box<dyn UsageProbe>>,
}

impl UsageService {
    pub fn new(probes: Vec<Box<dyn UsageProbe>>) -> Self {
        Self { probes }
    }

    pub fn collect_map(&self) -> UsageMap {
        let mut merged: UsageMap = HashMap::new();
        for probe in &self.probes {
            let Ok(usages) = probe.probe() else {
                continue;
            };
            for u in usages {
                match merged.get_mut(&u.exe_name) {
                    Some(existing) => {
                        if u.last_run_unix > existing.last_run_unix {
                            existing.last_run_unix = u.last_run_unix;
                            existing.source = u.source;
                        }
                        existing.run_count = existing.run_count.max(u.run_count);
                    }
                    None => {
                        merged.insert(u.exe_name.clone(), u);
                    }
                }
            }
        }
        merged
    }

    pub fn lookup<'a>(&self, map: &'a UsageMap, exe_name: &str) -> Option<&'a AppUsage> {
        map.get(&exe_name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::UsageSource;

    struct FixedProbe {
        entries: Vec<AppUsage>,
    }

    impl UsageProbe for FixedProbe {
        fn probe(&self) -> anyhow::Result<Vec<AppUsage>> {
            Ok(self.entries.clone())
        }
    }

    fn usage(name: &str, ts: i64, count: u64, src: UsageSource) -> AppUsage {
        AppUsage {
            exe_name: name.to_string(),
            last_run_unix: ts,
            run_count: count,
            source: src,
        }
    }

    #[test]
    fn merges_taking_latest_time_and_max_count() {
        let svc = UsageService::new(vec![
            Box::new(FixedProbe {
                entries: vec![usage("chrome.exe", 1000, 0, UsageSource::Prefetch)],
            }),
            Box::new(FixedProbe {
                entries: vec![
                    usage("chrome.exe", 2000, 55, UsageSource::UserAssist),
                    usage("code.exe", 500, 3, UsageSource::UserAssist),
                ],
            }),
            Box::new(FixedProbe {
                entries: vec![usage("broken", 1, 1, UsageSource::Prefetch)],
            }),
        ]);
        let map = svc.collect_map();

        assert_eq!(map.len(), 3);
        let chrome = map.get("chrome.exe").unwrap();
        assert_eq!(chrome.last_run_unix, 2000);
        assert_eq!(chrome.run_count, 55);
        assert_eq!(chrome.source, UsageSource::UserAssist);
        assert_eq!(map.get("broken").unwrap().last_run_unix, 1);

        assert!(svc.lookup(&map, "CODE.EXE").is_some());
        assert!(svc.lookup(&map, "missing.exe").is_none());
    }

    #[test]
    fn empty_and_failing_probes_are_tolerated() {
        struct FailingProbe;
        impl UsageProbe for FailingProbe {
            fn probe(&self) -> anyhow::Result<Vec<AppUsage>> {
                Err(anyhow::anyhow!("no access"))
            }
        }
        let svc = UsageService::new(vec![
            Box::new(FailingProbe),
            Box::new(FixedProbe { entries: vec![] }),
        ]);
        assert!(svc.collect_map().is_empty());
    }
}
