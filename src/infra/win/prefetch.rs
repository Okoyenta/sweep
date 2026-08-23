use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::domain::models::{AppUsage, UsageSource};
use crate::domain::traits::UsageProbe;

pub const PREFETCH_DIR: &str = "C:\\Windows\\Prefetch";

pub fn exe_from_pf_name(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".pf")?;
    let exe = stem.rsplit_once('-').map_or(stem, |(exe, _hash)| exe);
    if exe.is_empty() || !exe.contains('.') {
        return None;
    }
    Some(exe.to_lowercase())
}

fn mtime_secs(p: &PathBuf) -> Option<i64> {
    p.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

pub struct PrefetchProbe {
    dir: PathBuf,
}

impl PrefetchProbe {
    pub fn new() -> Self {
        Self {
            dir: PathBuf::from(PREFETCH_DIR),
        }
    }
}

impl Default for PrefetchProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageProbe for PrefetchProbe {
    fn probe(&self) -> anyhow::Result<Vec<AppUsage>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return Ok(out),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(exe) = exe_from_pf_name(&name) else {
                continue;
            };
            let Some(ts) = mtime_secs(&entry.path()) else {
                continue;
            };
            out.push(AppUsage {
                exe_name: exe,
                last_run_unix: ts,
                run_count: 0,
                source: UsageSource::Prefetch,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_exe_from_pf_names() {
        assert_eq!(
            exe_from_pf_name("CHROME.EXE-ABCDEF12.pf"),
            Some("chrome.exe".to_string())
        );
        assert_eq!(
            exe_from_pf_name("CODE.EXE-DEADBEEF.pf"),
            Some("code.exe".to_string())
        );
        assert_eq!(exe_from_pf_name("notapffile.txt"), None);
        assert_eq!(exe_from_pf_name(".pf"), None);
    }
}
