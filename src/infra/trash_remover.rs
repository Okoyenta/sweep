use std::path::{Component, Path, PathBuf};

use crate::domain::models::BinItem;
use crate::domain::traits::{PathRemover, RecycleBin};

pub struct TrashRemover;

impl TrashRemover {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TrashRemover {
    fn default() -> Self {
        Self::new()
    }
}

impl PathRemover for TrashRemover {
    fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        // Last line of defence: no built-in category is reclaimable under
        // System32, so whatever the caller thinks, refuse it here.
        if is_protected_system_path(path) {
            anyhow::bail!(
                "refusing to remove {}: it is under %SystemRoot%\\System32",
                path.display()
            );
        }
        trash::delete(path).map_err(|e| anyhow::anyhow!("trash delete failed: {e}"))
    }
}

/// True when `path` resolves to `%SystemRoot%\System32` or anything below it.
///
/// Case-insensitive, and resolved through `..`, junctions and 8.3 short names
/// (via the nearest existing ancestor) so spelling tricks cannot get around it.
/// Always false off Windows.
pub fn is_protected_system_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        let system32 = crate::domain::categories::system_root().join("System32");
        is_under(path, &system32)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn is_under(path: &Path, root: &Path) -> bool {
    let path = comparable(path);
    let root = comparable(root);
    path == root || path.starts_with(&format!("{root}\\"))
}

/// Lower-cased, backslash-separated, `\\?\`-free form of the resolved path.
fn comparable(path: &Path) -> String {
    let s = resolve(path)
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase();
    let s = s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s);
    s.trim_end_matches('\\').to_string()
}

/// Canonicalize the nearest existing ancestor and re-append the rest, so a
/// path that does not exist yet still resolves junctions above it.
fn resolve(path: &Path) -> PathBuf {
    let lexical = lexical_normalize(path);
    let mut tail = Vec::new();
    let mut cur = lexical.as_path();
    loop {
        if let Ok(canon) = std::fs::canonicalize(cur) {
            return tail.iter().rev().fold(canon, |acc, part| acc.join(part));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                cur = parent;
            }
            _ => return lexical,
        }
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub struct TrashBin;

impl TrashBin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TrashBin {
    fn default() -> Self {
        Self::new()
    }
}

impl RecycleBin for TrashBin {
    fn items(&self) -> anyhow::Result<Vec<BinItem>> {
        let items = trash::os_limited::list()
            .map_err(|e| anyhow::anyhow!("listing recycle bin failed: {e}"))?;
        Ok(items
            .into_iter()
            .map(|i| BinItem {
                name: i.name.to_string_lossy().into_owned(),
                original_parent: i.original_parent.to_string_lossy().into_owned(),
                deleted_unix: i.time_deleted,
            })
            .collect())
    }

    fn purge_all(&self) -> anyhow::Result<u64> {
        let items = trash::os_limited::list()
            .map_err(|e| anyhow::anyhow!("listing recycle bin failed: {e}"))?;
        let count = items.len() as u64;
        trash::os_limited::purge_all(items)
            .map_err(|e| anyhow::anyhow!("emptying recycle bin failed: {e}"))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn system32() -> PathBuf {
        crate::domain::categories::system_root().join("System32")
    }

    #[cfg(windows)]
    #[test]
    fn guard_refuses_driver_store_in_any_spelling() {
        let repo = crate::domain::categories::driver_store_path();
        assert!(is_protected_system_path(&repo));
        assert!(is_protected_system_path(&repo.join("no-such-package.inf_amd64_x")));
        let upper = PathBuf::from(repo.to_string_lossy().to_uppercase());
        assert!(is_protected_system_path(&upper));
        let dotted = system32().join("..").join("System32").join("drivers");
        assert!(is_protected_system_path(&dotted));
        let via_temp = crate::domain::categories::system_root()
            .join("Temp")
            .join("..")
            .join("System32")
            .join("DriverStore");
        assert!(is_protected_system_path(&via_temp));
        assert!(is_protected_system_path(&system32()));
    }

    #[cfg(windows)]
    #[test]
    fn guard_allows_ordinary_and_lookalike_paths() {
        assert!(!is_protected_system_path(&std::env::temp_dir().join("sweep-x")));
        let lookalike = crate::domain::categories::system_root().join("System32Extra");
        assert!(!is_protected_system_path(&lookalike));
    }

    #[cfg(windows)]
    #[test]
    fn trash_remover_refuses_system32_without_touching_it() {
        let target = crate::domain::categories::driver_store_path().join("sweep-guard-probe");
        let err = TrashRemover::new().remove_path(&target).unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
    }

    #[test]
    fn lexical_normalize_drops_dot_segments() {
        let p = lexical_normalize(Path::new("a/./b/../c"));
        assert_eq!(p, PathBuf::from("a").join("c"));
    }
}
