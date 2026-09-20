use std::path::{Path, PathBuf};

use crate::domain::models::{RESERVE_SIZE_BYTES, HEADROOM_THRESHOLD_BYTES, RECREATION_THRESHOLD_BYTES};

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            format!(
                "{}\\AppData\\Local",
                std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
            )
        });
        PathBuf::from(base).join("sweep")
    }
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("sweep");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".local/share/sweep")
    }
}

/// The user's home directory, resolved the way the platform's own tools resolve
/// it. On Windows this is `USERPROFILE`: a plain PowerShell / cmd / scheduled-task
/// launch never sets `HOME` (only POSIX-emulation shells such as Git Bash do), so
/// reading `HOME` would silently miss the profile for everyday users and the guard.
/// On other platforms it is `HOME`. Returns `None` when the variable is unset or
/// empty. Written by hand rather than via `std::env::home_dir`, which is deprecated
/// and whose exact behaviour varies by toolchain.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let profile = std::env::var("USERPROFILE").ok()?;
        if profile.is_empty() {
            None
        } else {
            Some(PathBuf::from(profile))
        }
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").ok()?;
        if home.is_empty() {
            None
        } else {
            Some(PathBuf::from(home))
        }
    }
}

pub fn index_db_path() -> PathBuf {
    if let Ok(val) = std::env::var("SWEEP_DB") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    data_dir().join("index.db")
}

pub fn reserve_path() -> PathBuf {
    if let Ok(val) = std::env::var("SWEEP_DB") {
        if !val.is_empty() {
            if let Some(parent) = PathBuf::from(&val).parent() {
                return parent.join("reserve.bin");
            }
        }
    }
    data_dir().join("reserve.bin")
}

/// Allocate the reserve file at full [`RESERVE_SIZE_BYTES`].
///
/// Deliberately private: this ignores free space entirely, so a command that
/// reclaims nothing — `sweep status`, `sweep index --status` — would re-occupy
/// space a `clean` had just freed, dropping the volume back under
/// [`HEADROOM_THRESHOLD_BYTES`] and re-arming the release/re-create cycle. Every
/// caller outside this module goes through [`try_recreate_reserve`], which gates
/// on [`RECREATION_THRESHOLD_BYTES`]; keeping this private makes reaching for it
/// directly a compile error rather than a silent regression.
fn ensure_reserve() -> anyhow::Result<u64> {
    let path = reserve_path();
    if path.exists() {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() >= RESERVE_SIZE_BYTES {
                return Ok(0);
            }
        }
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::File::create(&path)
        .map_err(|e| anyhow::anyhow!("creating reserve file: {e}"))?;
    let resized = file.set_len(RESERVE_SIZE_BYTES);
    // Close before cleanup: Windows will not unlink a file that still has an open
    // handle.
    drop(file);
    if let Err(e) = resized {
        // A failed `set_len` — the disk-full case this reserve exists for — leaves
        // a zero-length stub behind. Reporting that stub as a reserve is worse than
        // reporting nothing, so remove it and let the caller see the real error.
        let _ = std::fs::remove_file(&path);
        return Err(anyhow::anyhow!("setting reserve size: {e}"));
    }
    Ok(RESERVE_SIZE_BYTES)
}

pub fn consume_reserve() -> Option<u64> {
    let path = reserve_path();
    if !path.exists() {
        return None;
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    match std::fs::remove_file(&path) {
        Ok(()) => Some(size),
        Err(_) => None,
    }
}

pub fn has_reserve() -> bool {
    reserve_path().exists()
}

/// A mounted volume, and how much room it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpace {
    pub mount: PathBuf,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Resolve `path` to the volume that holds it, or `None` when none is mounted.
///
/// Matching is by mount-point prefix, then by the parent directory when the
/// immediate one matches nothing. A path below no mounted volume — a
/// `\\localhost\C$` share, say — resolves to `None` rather than to a volume it
/// is not on, which is what makes the reserve gate's "no space at all" reading
/// reachable in a test.
pub fn volume_of(path: &Path) -> Option<VolumeSpace> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mount = path.parent().unwrap_or(path);
    let hits = |candidate: &Path| {
        disks
            .list()
            .iter()
            .find(|d| {
                let mp = d.mount_point();
                mp == candidate || candidate.starts_with(mp) || mp.starts_with(candidate)
            })
            .map(|d| VolumeSpace {
                mount: d.mount_point().to_path_buf(),
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
            })
    };
    hits(mount).or_else(|| mount.parent().and_then(hits))
}

/// Summed capacity of the volumes the given paths live on, counting a volume
/// once however many paths land on it. `None` when no path resolves.
///
/// This is the denominator for "how much of the disk has the index seen": a
/// bare byte count cannot be judged without knowing what it is a share of.
pub fn volumes_total_for(paths: &[PathBuf]) -> Option<(u64, Vec<PathBuf>)> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut total = 0u64;
    for path in paths {
        let Some(v) = volume_of(path) else {
            continue;
        };
        if seen.contains(&v.mount) {
            continue;
        }
        seen.push(v.mount);
        total += v.total_bytes;
    }
    if seen.is_empty() {
        None
    } else {
        Some((total, seen))
    }
}

/// Free bytes on the volume holding the index, or 0 when nothing matches.
pub fn free_bytes_on_index_volume() -> u64 {
    volume_of(&index_db_path()).map_or(0, |v| v.available_bytes)
}

pub fn is_disk_full_error(err: &anyhow::Error) -> bool {
    let msg = format!("{err}");
    if msg.contains("disk I/O error") || msg.contains("No space left on device") {
        return true;
    }
    if let Some(sqlite_err) = err.downcast_ref::<rusqlite::Error>() {
        let s = format!("{sqlite_err}");
        if s.contains("SQLITE_FULL") || s.contains("13") {
            return true;
        }
    }
    false
}

/// Whether free space is low enough that the reserve must be released to make
/// room for the write that is about to happen.
fn should_consume(free_bytes: u64) -> bool {
    free_bytes < HEADROOM_THRESHOLD_BYTES
}

/// Whether free space has recovered enough to re-arm the reserve.
///
/// The gap between [`HEADROOM_THRESHOLD_BYTES`] and [`RECREATION_THRESHOLD_BYTES`]
/// is deliberate hysteresis: re-creating the moment free space clears the consume
/// threshold would immediately push the volume back under it, oscillating a
/// 512 MiB file between present and absent on every command.
fn should_recreate(free_bytes: u64) -> bool {
    free_bytes >= RECREATION_THRESHOLD_BYTES
}

pub fn ensure_headroom_or_consume_reserve() {
    if should_consume(free_bytes_on_index_volume()) {
        consume_reserve();
    }
}

pub fn try_recreate_reserve() {
    if should_recreate(free_bytes_on_index_volume()) {
        let _ = ensure_reserve();
    }
}

pub fn guard_log_path() -> PathBuf {
    data_dir().join("guard.log")
}

pub fn guard_lock_path() -> PathBuf {
    data_dir().join("guard.lock")
}

/// Path of the user `sweep.toml` config in the sweep data dir (used as the
/// lowest-precedence fallback after `--config` and the CWD copy).
pub fn sweep_toml_path() -> PathBuf {
    data_dir().join("sweep.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_reserve_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sweep-reserve-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ensure_reserve_creates_file_with_correct_size() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_reserve_dir();
        let path = dir.join("reserve.bin");
        let original = std::env::var("SWEEP_DB").ok();
        unsafe { std::env::set_var("SWEEP_DB", dir.join("index.db").to_str().unwrap()) };
        let result = ensure_reserve();
        assert!(result.is_ok());
        assert!(path.exists());
        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), RESERVE_SIZE_BYTES);
        let _ = fs::remove_dir_all(&dir);
        match original {
            Some(v) => unsafe { std::env::set_var("SWEEP_DB", v) },
            None => unsafe { std::env::remove_var("SWEEP_DB") },
        }
    }

    #[test]
    fn ensure_reserve_is_idempotent() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_reserve_dir();
        let original = std::env::var("SWEEP_DB").ok();
        unsafe { std::env::set_var("SWEEP_DB", dir.join("index.db").to_str().unwrap()) };
        let _ = ensure_reserve();
        let result = ensure_reserve();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
        match original {
            Some(v) => unsafe { std::env::set_var("SWEEP_DB", v) },
            None => unsafe { std::env::remove_var("SWEEP_DB") },
        }
    }

    #[test]
    fn consume_reserve_deletes_file_and_returns_size() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_reserve_dir();
        let reserve = dir.join("reserve.bin");
        std::fs::write(&reserve, vec![0u8; 100]).unwrap();
        let original = std::env::var("SWEEP_DB").ok();
        unsafe { std::env::set_var("SWEEP_DB", dir.join("index.db").to_str().unwrap()) };
        let result = consume_reserve();
        assert_eq!(result, Some(100));
        assert!(!reserve.exists());
        let _ = fs::remove_dir_all(&dir);
        match original {
            Some(v) => unsafe { std::env::set_var("SWEEP_DB", v) },
            None => unsafe { std::env::remove_var("SWEEP_DB") },
        }
    }

    #[test]
    fn consume_reserve_returns_none_when_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_reserve_dir();
        let original = std::env::var("SWEEP_DB").ok();
        unsafe { std::env::set_var("SWEEP_DB", dir.join("index.db").to_str().unwrap()) };
        let result = consume_reserve();
        assert_eq!(result, None);
        let _ = fs::remove_dir_all(&dir);
        match original {
            Some(v) => unsafe { std::env::set_var("SWEEP_DB", v) },
            None => unsafe { std::env::remove_var("SWEEP_DB") },
        }
    }

    #[test]
    fn sweep_db_env_overrides_index_db_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = temp_reserve_dir();
        let expected = dir.join("custom.db");
        let original = std::env::var("SWEEP_DB").ok();
        unsafe { std::env::set_var("SWEEP_DB", expected.to_str().unwrap()) };
        assert_eq!(index_db_path(), expected);
        assert!(reserve_path().ends_with("reserve.bin"));
        let _ = fs::remove_dir_all(&dir);
        match original {
            Some(v) => unsafe { std::env::set_var("SWEEP_DB", v) },
            None => unsafe { std::env::remove_var("SWEEP_DB") },
        }
    }

    // The bug this fixes: threshold decisions were inlined at each call site, so
    // the two sites that forgot to check at all were indistinguishable from the
    // ones that did. Pulling the comparison into a predicate makes the boundary
    // itself testable.

    #[test]
    fn consume_triggers_only_below_headroom() {
        assert!(should_consume(HEADROOM_THRESHOLD_BYTES - 1));
        assert!(!should_consume(HEADROOM_THRESHOLD_BYTES));
        assert!(!should_consume(RECREATION_THRESHOLD_BYTES));
    }

    #[test]
    fn recreate_triggers_only_at_or_above_the_recreation_threshold() {
        assert!(!should_recreate(0));
        assert!(!should_recreate(HEADROOM_THRESHOLD_BYTES));
        assert!(!should_recreate(RECREATION_THRESHOLD_BYTES - 1));
        assert!(should_recreate(RECREATION_THRESHOLD_BYTES));
    }

    /// A nearly-full disk must not re-arm the reserve: doing so would hand back
    /// space the next write needs, which is the whole failure mode being fixed.
    #[test]
    fn a_nearly_full_disk_does_not_recreate_the_reserve() {
        assert!(!should_recreate(237 * 1024 * 1024));
        // The band between the two thresholds is the hysteresis that stops the
        // 512 MiB file oscillating between present and absent on every command.
        for free in [
            HEADROOM_THRESHOLD_BYTES,
            HEADROOM_THRESHOLD_BYTES * 2,
            RECREATION_THRESHOLD_BYTES - 1,
        ] {
            assert!(
                !should_recreate(free),
                "{free} bytes free is inside the hysteresis band and must not re-arm"
            );
        }
    }
}
