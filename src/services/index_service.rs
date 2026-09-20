use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::models::{DirListing, IndexProgress, IndexStats};
use crate::domain::traits::IndexStore;
use crate::infra::walker::{default_roots, IncrementalWalker, WalkerConfig};

pub struct IndexConfig {
    pub roots: Vec<PathBuf>,
    pub walker: WalkerConfig,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            roots: default_roots(),
            walker: WalkerConfig::default(),
        }
    }
}

#[cfg(windows)]
fn set_background_priority(begin: bool) {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
        THREAD_MODE_BACKGROUND_END,
    };
    unsafe {
        let flag = if begin {
            THREAD_MODE_BACKGROUND_BEGIN
        } else {
            THREAD_MODE_BACKGROUND_END
        };
        SetThreadPriority(GetCurrentThread(), flag);
    }
}

#[cfg(not(windows))]
fn set_background_priority(_begin: bool) {}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What the last indexing run is entitled to speak for.
///
/// Every answer `dupes` and `status` give is an answer about this scope, not
/// about the disk: an index that is two weeks old and saw an eighth of the
/// volume will report `no duplicate groups found` with perfect confidence, and
/// a reader has no way to tell that from a real negative.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexProvenance {
    /// When the last run finished, or `None` if the index has never completed one.
    pub last_run_unix: Option<i64>,
    /// The roots that run walked, empty for an index built before they were recorded.
    pub roots: Vec<PathBuf>,
    /// The last run stopped before finishing, so its figures are a lower bound.
    pub interrupted: bool,
}

/// Read the provenance of the last run from any store.
pub fn read_provenance(store: &dyn IndexStore) -> anyhow::Result<IndexProvenance> {
    Ok(IndexProvenance {
        // An unparseable stamp is not a run we can date; degrade to "never".
        last_run_unix: store.meta_get("last_run")?.and_then(|v| v.parse().ok()),
        roots: store
            .meta_get("last_run_roots")?
            .filter(|v| !v.is_empty())
            .map(|v| v.split(';').map(PathBuf::from).collect())
            .unwrap_or_default(),
        interrupted: store.meta_get("last_run_interrupted")?.as_deref() == Some("1"),
    })
}

/// Roots as a single meta value. `;` cannot appear in a Windows path, and the
/// value is only ever read back to be displayed.
fn join_roots(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|r| r.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(";")
}

pub struct IndexService<S: IndexStore> {
    store: S,
    cfg: IndexConfig,
}

impl<S: IndexStore> IndexService<S> {
    pub fn new(store: S, cfg: IndexConfig) -> Self {
        Self { store, cfg }
    }

    pub fn stats(&self) -> anyhow::Result<IndexStats> {
        self.store.stats()
    }

    pub fn clear(&mut self) -> anyhow::Result<()> {
        self.store.clear()
    }

    pub fn last_run(&self) -> anyhow::Result<Option<i64>> {
        Ok(read_provenance(&self.store)?.last_run_unix)
    }

    /// What the last run covered, not just when it ran.
    pub fn provenance(&self) -> anyhow::Result<IndexProvenance> {
        read_provenance(&self.store)
    }

    pub fn run(
        &mut self,
        cancel: &AtomicBool,
        progress: &Mutex<IndexProgress>,
        on_tick: Option<&mut dyn FnMut(&IndexProgress)>,
    ) -> anyhow::Result<IndexProgress> {
        set_background_priority(true);
        let result = self.run_inner(cancel, progress, on_tick);
        set_background_priority(false);
        result
    }
    fn run_inner(
        &mut self,
        cancel: &AtomicBool,
        progress: &Mutex<IndexProgress>,
        mut on_tick: Option<&mut dyn FnMut(&IndexProgress)>,
    ) -> anyhow::Result<IndexProgress> {
        *progress.lock().unwrap() = IndexProgress {
            running: true,
            started_at_unix: Some(now_secs()),
            ..Default::default()
        };

        let mut walker = IncrementalWalker::new(self.cfg.roots.clone(), self.cfg.walker.clone());

        let mut interrupted = false;
        while let Some(listing) = walker.next_listing(&self.store) {
            if cancel.load(Ordering::Relaxed) {
                interrupted = true;
                break;
            }
            let readable = listing.readable;
            let entry_count = listing.entries.len();
            self.apply_listing(&listing)?;

            let mut p = progress.lock().unwrap();
            p.current_path = listing.path.clone();
            p.dirs_scanned += 1;
            p.files_recorded += entry_count as u64;
            if !readable {
                p.errors += 1;
            }
            drop(p);

            if let Some(f) = &mut on_tick {
                let snap = progress.lock().unwrap().clone();
                f(&snap);
            }
        }

        {
            let mut p = progress.lock().unwrap();
            p.running = false;
            p.dirs_skipped = walker.dirs_skipped();
            p.finished_at_unix = Some(now_secs());
        }
        // Record what the run covered as well as when it ran. Without a scope,
        // nothing downstream can tell a complete index from a partial one.
        self.store.meta_set("last_run", &now_secs().to_string())?;
        self.store
            .meta_set("last_run_roots", &join_roots(&self.cfg.roots))?;
        // A run stopped by `cancel` used to stamp `last_run` exactly as a
        // finished one did, so the provenance line would call a partial walk
        // complete.
        self.store
            .meta_set("last_run_interrupted", if interrupted { "1" } else { "0" })?;

        Ok(progress.lock().unwrap().clone())
    }

    fn apply_listing(&mut self, listing: &DirListing) -> anyhow::Result<()> {
        if listing.readable {
            let prior: Vec<String> = self
                .store
                .child_entries(&listing.path)?
                .into_iter()
                .map(|(p, _)| p)
                .collect();
            let current: HashSet<&str> = listing.entries.iter().map(|e| e.path.as_str()).collect();
            let stale: Vec<String> = prior
                .into_iter()
                .filter(|p| !current.contains(p.as_str()))
                .collect();
            if !stale.is_empty() {
                self.store.delete_paths(&stale)?;
            }
        }
        self.store.upsert_entries(&listing.entries)
    }
}

impl<S: IndexStore + Send + 'static> IndexService<S> {
    pub fn spawn(mut self) -> SpawnedIndex {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(IndexProgress::default()));
        let cancel_t = Arc::clone(&cancel);
        let progress_t = Arc::clone(&progress);
        let handle = std::thread::Builder::new()
            .name("sweep-indexer".to_string())
            .spawn(move || {
                let _ = self.run(&cancel_t, &progress_t, None);
            })
            .expect("spawning indexer thread");
        SpawnedIndex {
            handle,
            cancel,
            progress,
        }
    }
}

#[allow(dead_code)]
pub struct SpawnedIndex {
    pub handle: JoinHandle<()>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<IndexProgress>>,
}
