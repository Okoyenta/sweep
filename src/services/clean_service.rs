use std::path::Path;

use jwalk::WalkDir;

use crate::domain::models::{CategoryScan, CleanCategory, CleanOutcome};
use crate::domain::traits::PathRemover;

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
                }
            })
            .collect()
    }

    pub fn run(
        &self,
        scans: &[CategoryScan],
        only: Option<&[String]>,
    ) -> anyhow::Result<CleanOutcome> {
        let mut outcome = CleanOutcome::default();
        for scan in scans {
            if let Some(ids) = only {
                if !ids.iter().any(|id| id == &scan.category_id) {
                    continue;
                }
            }
            for item in &scan.items {
                let (bytes, _files) = measure(item);
                match self.remover.remove_path(item) {
                    Ok(()) => {
                        outcome.removed_items += 1;
                        outcome.removed_bytes += bytes;
                    }
                    Err(_) => {
                        outcome.failed_items += 1;
                    }
                }
            }
        }
        Ok(outcome)
    }
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
            },
            CategoryScan {
                category_id: "do".into(),
                title: "d".into(),
                items: vec![b.join("ok.bin"), b.join("locked.bin")],
                total_bytes: 0,
                files: 2,
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
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn missing_roots_are_ignored_in_scan() {
        let cats = vec![CleanCategory {
            id: "ghost".into(),
            title: "g".into(),
            roots: vec![std::env::temp_dir().join("sweep-does-not-exist-xyz")],
        }];
        let svc = CleanService::new(CountingRemover(AtomicUsize::new(0)));
        let scans = svc.scan(&cats);
        assert!(scans[0].items.is_empty());
        assert_eq!(scans[0].total_bytes, 0);
    }
}
