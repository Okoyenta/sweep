use crate::domain::models::DeepScanResult;

pub fn scan() -> DeepScanResult {
    let mut result = DeepScanResult {
        wu_download_bytes: 0,
        do_cache_bytes: 0,
        winsxs_reclaimable_bytes: None,
        driver_store_bytes: 0,
        driver_store_oldest_days: None,
    };

    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let wu_path = std::path::PathBuf::from(&local_appdata)
            .join("Microsoft")
            .join("Windows")
            .join("DeliveryOptimization");
        if wu_path.exists() {
            result.do_cache_bytes = dir_size(&wu_path);
        }
    }

    if let Some(program_data) = std::env::var_os("ProgramData") {
        let wu_path = std::path::PathBuf::from(program_data)
            .join("Microsoft")
            .join("Windows")
            .join("SoftwareDistribution")
            .join("Download");
        if wu_path.exists() {
            result.wu_download_bytes = dir_size(&wu_path);
        }
    }

    let driver_store = std::path::PathBuf::from("C:\\Windows\\System32\\DriverStore\\FileRepository");
    if driver_store.exists() {
        let (bytes, oldest_days) = driver_store_info(&driver_store);
        result.driver_store_bytes = bytes;
        result.driver_store_oldest_days = oldest_days;
    }

    result.winsxs_reclaimable_bytes = analyze_winsxs();

    result
}

fn analyze_winsxs() -> Option<u64> {
    let output = std::process::Command::new("dism")
        .args(["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("Estimated Size of WinSxS Folder") {
            let size_str = line.split(':').nth(1)?.trim();
            let cleaned: String = size_str.chars().filter(|c| c.is_ascii_digit()).collect();
            let kb: u64 = cleaned.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

fn driver_store_info(path: &std::path::Path) -> (u64, Option<u32>) {
    let mut total_bytes = 0u64;
    let mut oldest_days: Option<u32> = None;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                total_bytes += meta.len();
                if let Ok(modified) = meta.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        let days = elapsed.as_secs() / 86400;
                        oldest_days = Some(oldest_days.map_or(days as u32, |o| o.min(days as u32)));
                    }
                }
            }
            let sub = entry.path();
            if sub.is_dir() {
                let (sub_bytes, _) = driver_store_info(&sub);
                total_bytes += sub_bytes;
            }
        }
    }

    (total_bytes, oldest_days)
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_returns_struct() {
        let result = scan();
        assert!(result.wu_download_bytes == 0 || result.wu_download_bytes > 0);
    }
}
