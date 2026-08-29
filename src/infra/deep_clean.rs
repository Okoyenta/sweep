use crate::domain::models::DeepScanResult;

#[cfg(windows)]
pub fn deep_scan() -> DeepScanResult {
    super::win::deep_clean::scan()
}

#[cfg(not(windows))]
pub fn deep_scan() -> DeepScanResult {
    DeepScanResult {
        wu_download_bytes: 0,
        do_cache_bytes: 0,
        winsxs_reclaimable_bytes: None,
        driver_store_bytes: 0,
        driver_store_oldest_days: None,
    }
}
