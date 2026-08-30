use std::path::Path;

use jwalk::WalkDir;

use crate::domain::models::{
    CategoryScan, CleanCategory, CleanOutcome, ExclusionConfig, UndoItem,
};
use crate::domain::traits::PathRemover;

/// The cleanable categories for this run, plus how many were excluded.
///
/// Produced by [`discover_with_policy`] so every caller (clean, doctor, guard)
/// applies user exclusions and rule packs identically.
pub struct DiscoveredCategories {
    /// categories that survived exclusion filtering
    pub categories: Vec<CleanCategory>,
    /// how many categories `sweep.toml` excluded (for the `excluded: N` line)
    pub excluded: usize,
    /// the loaded exclusion config, for pruning individual items during scan
    pub exclusions: ExclusionConfig,
}

/// Discover built-in categories for this OS, merge user rule packs, then drop
/// everything excluded by `sweep.toml`.
///
/// This is the single discovery entry point used by `sweep clean`,
/// `sweep doctor`, and the guard disk rescue, so exclusions (FR-005) and rule
/// packs (FR-015) apply everywhere without duplication.
pub fn discover_with_policy(
    config_override: Option<&Path>,
    rules_override: Option<&Path>,
    deep: bool,
) -> DiscoveredCategories {
    use crate::services::exclusion_service;

    #[cfg(windows)]
    let builtin = if deep {
        crate::infra::win::clean_paths::discover_categories_deep()
    } else {
        crate::infra::win::clean_paths::discover_categories()
    };
    #[cfg(not(windows))]
    let builtin = crate::infra::linux::clean_paths::discover_categories();

    let (exclusions, packs) =
        exclusion_service::load_policy_with_rules(config_override, rules_override);

    let mut all = builtin.clone();
    all.extend(exclusion_service::rule_packs_to_categories(
        &packs, &builtin, deep,
    ));

    let (categories, excluded) = exclusion_service::apply_exclusions(&all, &exclusions);
    DiscoveredCategories {
        categories,
        excluded,
        exclusions,
    }
}

pub struct CleanService<R: PathRemover> {
    remover: R,
}

impl<R: PathRemover> CleanService<R> {
    pub fn new(remover: R) -> Self {
        Self { remover }
    }

    /// measures every category: candidates = existing children of each root
    /// (or the root itself when it is a plain file)
    pub fn scan(&self, categories: &[CleanCategory]) -> Vec<CategoryScan> {
        self.scan_excluding(categories, &ExclusionConfig::default())
    }

    /// Like [`Self::scan`], but prunes individual candidate items matched by
    /// `excl` *before* their size is measured, so excluded space is never
    /// counted and never cleaned (FR-005, US2 acceptance #4).
    pub fn scan_excluding(
        &self,
        categories: &[CleanCategory],
        excl: &ExclusionConfig,
    ) -> Vec<CategoryScan> {
        use crate::services::exclusion_service::is_path_excluded;

        categories
            .iter()
            .map(|cat| {
                let mut items = Vec::new();
                for root in &cat.roots {
                    if root.is_file() {
                        items.push(root.clone());
                    } else if root.is_dir() {
                        match std::fs::read_dir(root) {
                            Ok(entries) => {
                                items.extend(entries.flatten().map(|e| e.path()));
                            }
                            Err(_) => {}
                        }
                    }
                }
                items.retain(|item| !is_path_excluded(item, excl));
                let mut total_bytes = 0u64;
                let mut files = 0u64;
                for item in &items {
                    let (b, f) = measure(item);
                    total_bytes += b;
                    files += f;
                }
                CategoryScan {
                    category_id: cat.id.clone(),
                    title: cat.title.clone(),
                    files,
                    total_bytes,
                    items,
                    cleanup_command: cat.cleanup_command.clone(),
                }
            })
            .collect()
    }

    /// Size categories under a wall-clock `budget`, giving up on further walking
    /// once it is spent.
    ///
    /// A full scan walks every cache tree and can take minutes on a large disk,
    /// which `sweep doctor` cannot afford (SC-001: under 5 seconds). Returns the
    /// scans plus a flag indicating the budget ran out, so the caller can label
    /// the figure as partial rather than reporting a wrong total as exact.
    pub fn scan_within(
        &self,
        categories: &[CleanCategory],
        excl: &ExclusionConfig,
        budget: std::time::Duration,
    ) -> (Vec<CategoryScan>, bool) {
        use crate::services::exclusion_service::is_path_excluded;

        let deadline = std::time::Instant::now() + budget;
        let mut truncated = false;
        let scans = categories
            .iter()
            .map(|cat| {
                let mut items = Vec::new();
                for root in &cat.roots {
                    if root.is_file() {
                        items.push(root.clone());
                    } else if root.is_dir() {
                        if let Ok(entries) = std::fs::read_dir(root) {
                            items.extend(entries.flatten().map(|e| e.path()));
                        }
                    }
                }
                items.retain(|item| !is_path_excluded(item, excl));
                let mut total_bytes = 0u64;
                let mut files = 0u64;
                for item in &items {
                    let (b, f, done) = measure_within(item, deadline);
                    total_bytes += b;
                    files += f;
                    if !done {
                        truncated = true;
                    }
                }
                CategoryScan {
                    category_id: cat.id.clone(),
                    title: cat.title.clone(),
                    files,
                    total_bytes,
                    items,
                    cleanup_command: cat.cleanup_command.clone(),
                }
            })
            .collect();
        (scans, truncated)
    }

    pub fn run(
        &self,
        scans: &[CategoryScan],
        only: Option<&[String]>,
    ) -> anyhow::Result<CleanOutcome> {
        let mut outcome = CleanOutcome::default();
        for scan in scans {
            if let Some(ids) = only {
                if ids.is_empty() {
                    // empty vec = clean all, same as None
                } else if !ids.iter().any(|id| id == &scan.category_id) {
                    continue;
                }
            }
            if let Some(ref cmd) = scan.cleanup_command {
                let status = if cfg!(windows) {
                    std::process::Command::new("cmd")
                        .args(["/C", cmd])
                        .status()
                } else {
                    std::process::Command::new("sh")
                        .args(["-c", cmd])
                        .status()
                };
                match status {
                    Ok(st) if st.success() => {
                        outcome.removed_items += 1;
                        outcome.removed_bytes += scan.total_bytes;
                    }
                    _ => {
                        outcome.failed_items += scan.items.len() as u64;
                        outcome.failed_paths.extend(scan.items.iter().cloned());
                    }
                }
                continue;
            }
            for item in &scan.items {
                let (bytes, _files) = measure(item);
                match self.remover.remove_path(item) {
                    Ok(()) => {
                        outcome.removed_items += 1;
                        outcome.removed_bytes += bytes;
                        // Record the move so `sweep undo` can restore it. The
                        // trash crate exposes no separate trash id, so the
                        // original path doubles as the lookup key (FR-006).
                        outcome.undo_items.push(UndoItem {
                            original_path: item.clone(),
                            trash_path: item.clone(),
                            size_bytes: bytes,
                        });
                    }
                    Err(_) => {
                        outcome.failed_items += 1;
                        outcome.failed_paths.push(item.clone());
                    }
                }
            }
        }
        Ok(outcome)
    }
}

/// Like [`measure`], but stops walking once `deadline` passes.
///
/// Returns `(bytes, files, completed)`; `completed` is false when the walk was
/// cut short, so the caller knows the figure is a lower bound.
fn measure_within(path: &Path, deadline: std::time::Instant) -> (u64, u64, bool) {
    if std::time::Instant::now() >= deadline {
        return (0, 0, false);
    }
    if path.is_file() {
        return (
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            1,
            true,
        );
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    // Check the clock every N entries rather than per entry: the syscall is
    // cheap but not free, and cache trees have many small files.
    const CLOCK_CHECK_EVERY: u64 = 512;
    let mut seen = 0u64;
    for entry in WalkDir::new(path).parallelism(jwalk::Parallelism::Serial) {
        seen += 1;
        if seen % CLOCK_CHECK_EVERY == 0 && std::time::Instant::now() >= deadline {
            return (bytes, files, false);
        }
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                bytes += md.len();
                files += 1;
            }
        }
    }
    (bytes, files, true)
}

fn measure(path: &Path) -> (u64, u64) {
    if path.is_file() {
        return (
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            1,
        );
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(path).parallelism(jwalk::Parallelism::Serial) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                bytes += md.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct RecordingRemover {
        removed: RefCell<Vec<String>>,
        fail_on: Vec<String>,
    }

    impl PathRemover for RecordingRemover {
        fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
            let s = path.to_string_lossy().to_string();
            if self.fail_on.iter().any(|f| s.contains(f)) {
                anyhow::bail!("locked");
            }
            self.removed.borrow_mut().push(s);
            Ok(())
        }
    }

    struct CountingRemover(AtomicUsize);
    impl PathRemover for CountingRemover {
        fn remove_path(&self, _p: &Path) -> anyhow::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn temp_tree(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("sweep-clean-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(base.join("sub").join("b.bin"), vec![0u8; 50]).unwrap();
        base
    }

    #[test]
    fn scan_measures_children_and_files() {
        let base = temp_tree("scan");
        let cats = vec![CleanCategory {
            id: "t".into(),
            title: "test".into(),
            roots: vec![base.clone()],
            risk: crate::domain::models::RiskLevel::Safe,
            cleanup_command: None,
        }];
        let svc = CleanService::new(CountingRemover(AtomicUsize::new(0)));
        let scans = svc.scan(&cats);
        assert_eq!(scans[0].items.len(), 2); // a.bin + sub/
        assert_eq!(scans[0].files, 2);
        assert_eq!(scans[0].total_bytes, 150);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn run_filters_by_category_ids_and_counts_failures() {
        let a = temp_tree("run-a");
        let b = temp_tree("run-b");
        let scans = vec![
            CategoryScan {
                category_id: "keep".into(),
                title: "k".into(),
                items: vec![a.join("a.bin")],
                total_bytes: 100,
                files: 1,
                cleanup_command: None,
            },
            CategoryScan {
                category_id: "do".into(),
                title: "d".into(),
                items: vec![b.join("ok.bin"), b.join("locked.bin")],
                total_bytes: 0,
                files: 2,
                cleanup_command: None,
            },
        ];
        let remover = RecordingRemover {
            removed: RefCell::new(vec![]),
            fail_on: vec!["locked".into()],
        };
        let svc = CleanService::new(remover);
        let out = svc.run(&scans, Some(&["do".into()])).unwrap();

        assert_eq!(out.removed_items, 1);
        assert_eq!(out.failed_items, 1);
        assert_eq!(out.failed_paths, vec![b.join("locked.bin")]);
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn missing_roots_are_ignored_in_scan() {
        let cats = vec![CleanCategory {
            id: "ghost".into(),
            title: "g".into(),
            roots: vec![std::env::temp_dir().join("sweep-does-not-exist-xyz")],
            risk: crate::domain::models::RiskLevel::Safe,
            cleanup_command: None,
        }];
        let svc = CleanService::new(CountingRemover(AtomicUsize::new(0)));
        let scans = svc.scan(&cats);
        assert!(scans[0].items.is_empty());
        assert_eq!(scans[0].total_bytes, 0);
    }
}
