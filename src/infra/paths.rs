use std::path::PathBuf;

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
    data_dir().join("index.db")
}
