use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Duration;

use sweep::infra::sqlite_store::SqliteStore;
use sweep::infra::walker::{default_excludes, WalkerConfig};
use sweep::services::index_service::{IndexConfig, IndexService};

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("sweep-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        TempTree { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn write(&self, rel: &str, contents: &str) {
        if let Some(parent) = self.path(rel).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(self.path(rel), contents).unwrap();
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.path(rel)).unwrap();
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn service_for(root: &Path, store: SqliteStore) -> IndexService<SqliteStore> {
    IndexService::new(
        store,
        IndexConfig {
            roots: vec![root.to_path_buf()],
            walker: WalkerConfig {
                excludes: default_excludes(),
                pause_every_dirs: 1000,
                pause_for_ms: 0,
            },
        },
    )
}

fn run_index(svc: &mut IndexService<SqliteStore>) -> sweep::domain::models::IndexProgress {
    svc.run(
        &AtomicBool::new(false),
        &Mutex::new(Default::default()),
        None,
    )
    .unwrap()
}

fn settle() {
    std::thread::sleep(Duration::from_millis(1200));
}

#[test]
fn full_incremental_cycle() {
    let tree = TempTree::new("cycle");
    tree.write("a.txt", "0123456789");
    tree.write("sub/b.txt", "01234567890123456789");
    tree.write("sub/deep/c.txt", "12345");
    settle();

    let mut svc = service_for(&tree.root, SqliteStore::open_in_memory().unwrap());

    let p1 = run_index(&mut svc);
    assert!(p1.dirs_scanned >= 3, "root+sub+deep should be scanned");
    let s1 = svc.stats().unwrap();
    assert_eq!(s1.files, 3);
    assert_eq!(s1.total_bytes, 35);
    assert!(s1.dirs >= 3);
    assert!(p1.errors == 0);

    let p2 = run_index(&mut svc);
    assert_eq!(
        p2.dirs_scanned, 0,
        "unchanged tree must be fully skipped"
    );
    let p2_skipped = p2.dirs_skipped;
    assert!(p2_skipped >= 3);

    tree.write("sub/d.txt", "xyz");
    settle();
    let p3 = run_index(&mut svc);
    assert_eq!(p3.dirs_scanned, 1, "only 'sub' changed (d.txt added)");
    let s3 = svc.stats().unwrap();
    assert_eq!(s3.files, 4);
    assert_eq!(s3.total_bytes, 38);

    tree.remove("a.txt");
    settle();
    let p4 = run_index(&mut svc);
    assert!(p4.dirs_scanned >= 1);
    let s4 = svc.stats().unwrap();
    assert_eq!(s4.files, 3);
    assert_eq!(s4.total_bytes, 28);

    let last_run = svc.last_run().unwrap();
    assert!(last_run.is_some());
}

/// A finished run records what it covered, so a later answer can say how much
/// of the disk it is entitled to speak for.
#[test]
fn a_completed_run_records_its_roots_and_is_not_interrupted() {
    let tree = TempTree::new("provenance");
    tree.write("a.txt", "0123456789");
    settle();

    let mut svc = service_for(&tree.root, SqliteStore::open_in_memory().unwrap());
    run_index(&mut svc);

    let prov = svc.provenance().unwrap();
    assert!(prov.last_run_unix.is_some());
    assert_eq!(prov.roots, vec![tree.root.clone()]);
    assert!(!prov.interrupted, "a run that finished is not partial");
}

/// The bug this pins down: a run stopped by `cancel` stamped `last_run` exactly
/// as a finished one did, so the provenance line would call a partial walk
/// complete and present a lower bound as the answer.
#[test]
fn a_cancelled_run_is_recorded_as_interrupted() {
    let tree = TempTree::new("cancelled");
    tree.write("a.txt", "0123456789");
    settle();

    let mut svc = service_for(&tree.root, SqliteStore::open_in_memory().unwrap());
    let cancel = AtomicBool::new(true); // cancelled before the first listing
    svc.run(&cancel, &Mutex::new(Default::default()), None)
        .unwrap();

    let prov = svc.provenance().unwrap();
    assert!(
        prov.interrupted,
        "a cancelled run must not be reported as a completed one"
    );
}

/// An index built before the scope was recorded has no roots, which is the
/// signal the provenance line uses to decline to claim a share of the disk.
#[test]
fn provenance_without_recorded_roots_reports_an_empty_scope() {
    let store = SqliteStore::open_in_memory().unwrap();
    let prov = sweep::services::index_service::read_provenance(&store).unwrap();
    assert_eq!(prov.last_run_unix, None);
    assert!(prov.roots.is_empty());
    assert!(!prov.interrupted);
}

#[test]
fn walker_respects_excludes_and_missing_roots() {
    let tree = TempTree::new("exclude");
    tree.write("keep.txt", "k");
    tree.write("$recycle.bin/junk.txt", "junk");
    settle();

    let mut svc = service_for(&tree.root, SqliteStore::open_in_memory().unwrap());
    run_index(&mut svc);
    let s = svc.stats().unwrap();
    assert_eq!(s.files, 1, "excluded subtree must not be indexed");

    let empty_root = tree.path("does-not-exist");
    let mut svc2 = service_for(&empty_root, SqliteStore::open_in_memory().unwrap());
    let p = run_index(&mut svc2);
    assert_eq!(p.dirs_scanned, 0);
}
