use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::domain::models::{DirListing, EntryRecord};
use crate::domain::traits::IndexStore;

#[derive(Debug, Clone)]
pub struct WalkerConfig {
    pub excludes: Vec<String>,
    pub pause_every_dirs: u32,
    pub pause_for_ms: u64,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            excludes: default_excludes(),
            pause_every_dirs: 25,
            pause_for_ms: 5,
        }
    }
}

pub fn default_excludes() -> Vec<String> {
    vec![
        "$recycle.bin".to_string(),
        "system volume information".to_string(),
        "windows.old".to_string(),
    ]
}

pub fn default_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .list()
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| d.mount_point().to_path_buf())
            .filter(|p| p.to_string_lossy().len() <= 3 && p.to_string_lossy().ends_with('\\'))
            .collect()
    }
    #[cfg(not(windows))]
    {
        match std::env::var("HOME") {
            Ok(h) if !h.is_empty() => vec![PathBuf::from(h)],
            _ => vec![PathBuf::from("/")],
        }
    }
}

pub fn norm(path: &Path) -> String {
    let s = path
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn norm_parent(path_str: &str) -> String {
    Path::new(path_str)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_excluded(dir_norm: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|x| dir_norm.ends_with(x))
}

pub struct IncrementalWalker {
    queue: VecDeque<PathBuf>,
    cfg: WalkerConfig,
    dirs_since_pause: u32,
    dirs_skipped: u64,
}

impl IncrementalWalker {
    pub fn new(roots: Vec<PathBuf>, cfg: WalkerConfig) -> Self {
        let mut queue = VecDeque::new();
        for r in roots {
            if r.is_dir() {
                queue.push_back(r);
            }
        }
        Self {
            queue,
            cfg,
            dirs_since_pause: 0,
            dirs_skipped: 0,
        }
    }

    pub fn dirs_skipped(&self) -> u64 {
        self.dirs_skipped
    }

    fn maybe_pause(&mut self) {
        self.dirs_since_pause += 1;
        if self.dirs_since_pause >= self.cfg.pause_every_dirs {
            self.dirs_since_pause = 0;
            std::thread::sleep(Duration::from_millis(self.cfg.pause_for_ms));
        }
    }

    fn scan_dir(&mut self, dir: &Path) -> DirListing {
        let dn = norm(dir);
        let dir_mtime = std::fs::metadata(dir)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let mut listing = DirListing {
            path: dn.clone(),
            dir_mtime_ms: dir_mtime,
            readable: false,
            entries: vec![EntryRecord {
                path: dn.clone(),
                parent: norm_parent(&dn),
                size_bytes: 0,
                mtime_ms: dir_mtime,
                is_dir: true,
            }],
        };

        let read = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return listing,
        };
        listing.readable = true;

        for entry in read.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }

            let (size, mtime) = match entry.metadata() {
                Ok(m) => (
                    m.len(),
                    m.modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(-1),
                ),
                Err(_) => (0, -1),
            };

            let npath = norm(&path);
            let is_dir = file_type.is_dir();
            if is_dir {
                self.queue.push_back(path);
            }
            listing.entries.push(EntryRecord {
                path: npath,
                parent: dn.clone(),
                size_bytes: size,
                mtime_ms: mtime,
                is_dir,
            });
        }

        listing
    }
}

impl IncrementalWalker {
    pub fn next_listing(&mut self, store: &dyn IndexStore) -> Option<DirListing> {
        loop {
            let dir = self.queue.pop_front()?;
            let dn = norm(&dir);
            if is_excluded(&dn, &self.cfg.excludes) {
                continue;
            }

            let cur_mtime = std::fs::metadata(&dir)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);

            let Some(cur_mtime) = cur_mtime else { continue };

            if let Ok(Some(cached)) = store.get_dir_mtime(&dn) {
                if cached == cur_mtime {
                    match store.child_entries(&dn) {
                        Ok(children) if !children.is_empty() => {
                            for (path, is_dir) in children {
                                if is_dir {
                                    self.queue.push_back(PathBuf::from(path));
                                }
                            }
                            self.dirs_skipped += 1;
                            self.maybe_pause();
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            self.maybe_pause();
            return Some(self.scan_dir(&dir));
        }
    }
}
