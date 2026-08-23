use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;
use std::io::Read;

use crate::domain::traits::IndexStore;

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub size_bytes: u64,
    /// ordered oldest-first by indexed mtime is not available here; paths
    /// are sorted ascending, the first one is treated as the "keeper"
    pub paths: Vec<String>,
    pub wasted_bytes: u64,
}

/// hashes file contents in chunks; hash values are only meaningful within a
/// single process run (DefaultHasher is not stable across versions)
#[derive(Default)]
pub struct StdFileHasher;

impl StdFileHasher {
    pub fn new() -> Self {
        Self
    }

    fn hash_one(path: &str) -> anyhow::Result<u64> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = DefaultHasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.write(&buf[..n]);
        }
        Ok(hasher.finish())
    }
}

pub trait FileHasher {
    fn hash_file(&self, path: &str) -> anyhow::Result<u64>;
}

impl FileHasher for StdFileHasher {
    fn hash_file(&self, path: &str) -> anyhow::Result<u64> {
        Self::hash_one(path)
    }
}

pub struct DupFinder<S: IndexStore, H: FileHasher> {
    store: S,
    hasher: H,
}

impl<S: IndexStore, H: FileHasher> DupFinder<S, H> {
    pub fn new(store: S, hasher: H) -> Self {
        Self { store, hasher }
    }

    /// finds duplicate file groups among indexed files >= min_size bytes.
    /// groups are sorted by wasted bytes desc and capped at max_groups.
    pub fn find(
        &self,
        min_size: u64,
        max_groups: usize,
    ) -> anyhow::Result<Vec<DuplicateGroup>> {
        let files = self.store.files_by_size(min_size)?;

        // bucket by size first (cheap); only same-size files can be dupes
        let mut by_size: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        for (path, size) in files {
            by_size.entry(size).or_default().push(path);
        }

        let mut groups: Vec<DuplicateGroup> = Vec::new();
        for (size, mut paths) in by_size.into_iter().rev() {
            if groups.len() >= max_groups {
                break;
            }
            if size == 0 || paths.len() < 2 {
                continue;
            }
            paths.sort();
            let mut buckets: HashMap<u64, Vec<String>> = HashMap::new();
            for p in &paths {
                match self.hasher.hash_file(p) {
                    Ok(h) => buckets.entry(h).or_default().push(p.clone()),
                    Err(_) => {} // unreadable/deleted since indexing: skip
                }
            }
            for (_, mut same) in buckets {
                if same.len() < 2 {
                    continue;
                }
                same.sort();
                groups.push(DuplicateGroup {
                    size_bytes: size,
                    wasted_bytes: size * (same.len() as u64 - 1),
                    paths: same,
                });
            }
        }

        groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
        groups.truncate(max_groups);
        Ok(groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeStore {
        rows: Vec<(String, u64)>,
    }

    impl IndexStore for FakeStore {
        fn get_dir_mtime(&self, _: &str) -> anyhow::Result<Option<i64>> {
            Ok(None)
        }
        fn child_entries(&self, _: &str) -> anyhow::Result<Vec<(String, bool)>> {
            Ok(vec![])
        }
        fn upsert_entries(&mut self, _: &[crate::domain::models::EntryRecord]) -> anyhow::Result<()> {
            Ok(())
        }
        fn delete_paths(&mut self, _: &[String]) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn stats(&self) -> anyhow::Result<crate::domain::models::IndexStats> {
            Ok(Default::default())
        }
        fn clear(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        fn meta_get(&self, _: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
        fn meta_set(&mut self, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn files_by_size(&self, min_size: u64) -> anyhow::Result<Vec<(String, u64)>> {
            Ok(self
                .rows
                .iter()
                .filter(|(_, s)| *s >= min_size)
                .cloned()
                .collect())
        }
    }

    /// paths containing "same<N>" share a hash; others are unique
    struct FakeHasher;
    impl FileHasher for FakeHasher {
        fn hash_file(&self, path: &str) -> anyhow::Result<u64> {
            Ok(if path.contains("same1") {
                111
            } else if path.contains("same2") {
                222
            } else {
                let mut h = DefaultHasher::new();
                h.write(path.as_bytes());
                h.finish()
            })
        }
    }

    fn finder(rows: Vec<(String, u64)>) -> DupFinder<FakeStore, FakeHasher> {
        DupFinder::new(FakeStore { rows }, FakeHasher)
    }

    #[test]
    fn groups_same_size_same_hash_only() {
        let f = finder(vec![
            ("C:\\a\\same1\\x.iso".into(), 100),
            ("C:\\b\\same1\\y.iso".into(), 100),
            ("C:\\c\\same2\\z.bin".into(), 100), // different content hash
            ("C:\\d\\w.txt".into(), 100),
            ("C:\\e\\v.log".into(), 50), // unique anyway
        ]);
        let g = f.find(10, 50).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(
            g[0].paths,
            vec![
                "C:\\a\\same1\\x.iso".to_string(),
                "C:\\b\\same1\\y.iso".to_string()
            ]
        );
        assert_eq!(g[0].wasted_bytes, 100);
        assert_eq!(g[0].size_bytes, 100);
    }

    #[test]
    fn min_size_filters_and_zero_sizes_ignored() {
        let f = finder(vec![
            ("C:\\zero1".into(), 0),
            ("C:\\zero2".into(), 0),
            ("C:\\tiny1".into(), 3),
            ("C:\\tiny2".into(), 3),
        ]);
        assert!(f.find(5, 50).unwrap().is_empty());
        assert!(f.find(1, 50).unwrap().is_empty());
    }

    #[test]
    fn multiple_groups_sorted_by_wasted_desc_and_capped() {
        let rows: Vec<(String, u64)> = (0..3)
            .map(|i| (format!("C:\\l\\same2\\big{i}.iso"), 1000))
            .chain((0..4).map(|i| (format!("C:\\s\\same1\\small{i}.bin"), 10)))
            .collect();
        let f = finder(rows.clone());
        let g = f.find(1, 10).unwrap();
        assert_eq!(g[0].size_bytes, 1000);
        assert_eq!(g[0].wasted_bytes, 2000);
        assert_eq!(g[1].size_bytes, 10);
        assert_eq!(g[1].wasted_bytes, 30);

        let f2 = finder(rows);
        let capped = f2.find(1, 1).unwrap();
        assert_eq!(capped.len(), 1);
    }

    #[test]
    fn unreadable_files_are_skipped_not_fatal() {
        struct BrokenHasher;
        impl FileHasher for BrokenHasher {
            fn hash_file(&self, _: &str) -> anyhow::Result<u64> {
                Err(anyhow::anyhow!("gone"))
            }
        }
        let f = DupFinder::new(
            FakeStore {
                rows: vec![("C:\\x".into(), 9), ("C:\\y".into(), 9)],
            },
            BrokenHasher,
        );
        assert!(f.find(1, 10).unwrap().is_empty());
    }
}
