use std::io::Write;

use crate::domain::traits::RamTrimmer;

pub struct LinuxRamTrimmer;

impl LinuxRamTrimmer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxRamTrimmer {
    fn default() -> Self {
        Self::new()
    }
}

/// drops page cache / dentries / inodes (needs root);
/// per-process working-set trimming has no portable Linux equivalent
impl RamTrimmer for LinuxRamTrimmer {
    fn trim_processes(&mut self, _pids: &[u32]) -> anyhow::Result<(u32, u32)> {
        anyhow::bail!("per-process working-set trim is not supported on linux")
    }

    fn purge_standby(&mut self) -> anyhow::Result<bool> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open("/proc/sys/vm/drop_caches")
            .map_err(|e| {
                anyhow::anyhow!(
                    "cannot open drop_caches ({e}); run as root to purge kernel caches"
                )
            })?;
        f.write_all(b"3\n")
            .map_err(|e| anyhow::anyhow!("writing drop_caches failed: {e}"))?;
        Ok(true)
    }
}
