use std::path::PathBuf;

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

pub fn ensure_reserve() -> anyhow::Result<u64> {
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
    file.set_len(RESERVE_SIZE_BYTES)
        .map_err(|e| anyhow::anyhow!("setting reserve size: {e}"))?;
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

pub fn free_bytes_on_index_volume() -> u64 {
    let db = index_db_path();
    let mount = db.parent().unwrap_or(&db);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    for disk in disks.list() {
        let mp = disk.mount_point();
        if mp == mount || mount.starts_with(mp) || mp.starts_with(mount) {
            return disk.available_space();
        }
    }
    if let Some(parent) = mount.parent() {
        for disk in disks.list() {
            let mp = disk.mount_point();
            if parent == mp || parent.starts_with(mp) || mp.starts_with(parent) {
                return disk.available_space();
            }
        }
    }
    0
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

pub fn ensure_headroom_or_consume_reserve() {
    if free_bytes_on_index_volume() < HEADROOM_THRESHOLD_BYTES {
        consume_reserve();
    }
}

pub fn try_recreate_reserve() {
    if free_bytes_on_index_volume() >= RECREATION_THRESHOLD_BYTES {
        let _ = ensure_reserve();
    }
}

pub fn guard_log_path() -> PathBuf {
    data_dir().join("guard.log")
}

pub fn guard_lock_path() -> PathBuf {
    data_dir().join("guard.lock")
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
}
