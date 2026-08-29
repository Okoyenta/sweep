use sysinfo::{ProcessesToUpdate, System};

use crate::domain::models::{IdleReason, IdleSsdOffender};

pub struct IdleConfig {
    pub top: usize,
    pub idle_mins: u64,
    pub min_write_mb: u64,
    pub clean_cache: bool,
}

pub struct IdleService;

impl IdleService {
    pub fn new() -> Self {
        Self
    }

    pub fn detect(&self, config: &IdleConfig) -> anyhow::Result<Vec<IdleSsdOffender>> {
        let mut sys1 = System::new();
        sys1.refresh_processes(ProcessesToUpdate::All, true);
        std::thread::sleep(std::time::Duration::from_secs(60));
        let mut sys2 = System::new();
        sys2.refresh_processes(ProcessesToUpdate::All, true);

        let fg_pid = foreground_pid();
        let min_write_bytes = config.min_write_mb * 1024 * 1024;
        let min_idle_secs = config.idle_mins * 60;

        let mut offenders = Vec::new();

        for (pid, proc2) in sys2.processes() {
            if pid.as_u32() == fg_pid {
                continue;
            }
            let proc1 = sys1.process(*pid);
            let write_delta = proc2
                .disk_usage()
                .total_written_bytes
                .saturating_sub(
                    proc1
                        .map(|p| p.disk_usage().total_written_bytes)
                        .unwrap_or(proc2.disk_usage().total_written_bytes),
                );

            if write_delta < min_write_bytes {
                continue;
            }

            let idle_secs = proc2
                .run_time();
            if idle_secs < min_idle_secs {
                continue;
            }

            let writes_per_hour = if idle_secs > 0 {
                (write_delta as f64 / idle_secs as f64) * 3600.0
            } else {
                0.0
            };

            let reason = classify_reason(&proc2.name().to_string_lossy());

            offenders.push(IdleSsdOffender {
                pid: pid.as_u32(),
                name: proc2.name().to_string_lossy().into_owned(),
                idle_secs,
                write_bytes: write_delta,
                writes_per_hour,
                memory_bytes: proc2.memory(),
                reason,
            });
        }

        offenders.sort_by(|a, b| b.write_bytes.cmp(&a.write_bytes));
        offenders.truncate(config.top);
        Ok(offenders)
    }

    pub fn clean_cache(offenders: &[IdleSsdOffender]) -> anyhow::Result<u64> {
        use crate::domain::traits::PathRemover;
        use crate::infra::trash_remover::TrashRemover;

        let remover = TrashRemover::new();
        let mut freed = 0u64;

        for off in offenders {
            if let Some(cachedirs) = cache_dirs_for_process(&off.name) {
                for dir in &cachedirs {
                    if dir.exists() {
                        if let Ok(meta) = std::fs::metadata(dir) {
                            freed += meta.len();
                        }
                        let _ = remover.remove_path(dir);
                    }
                }
            }
        }

        Ok(freed)
    }
}

fn foreground_pid() -> u32 {
    std::process::id()
}

fn classify_reason(name: &str) -> IdleReason {
    let lower = name.to_lowercase();
    if lower.contains("sync") || lower.contains("onedrive") || lower.contains("dropbox") {
        IdleReason::SyncService
    } else if lower.contains("flush") || lower.contains("write") || lower.contains("cache") {
        IdleReason::BackgroundFlush
    } else {
        IdleReason::Unknown
    }
}

fn cache_dirs_for_process(name: &str) -> Option<Vec<std::path::PathBuf>> {
    let mut dirs = Vec::new();
    let lower = name.to_lowercase();

    if lower.contains("chrome") || lower.contains("msedge") {
        if let Ok(lad) = std::env::var("LOCALAPPDATA") {
            let base = std::path::PathBuf::from(lad);
            if lower.contains("chrome") {
                dirs.push(base.join("Google/Chrome/User Data/Default/Cache"));
                dirs.push(base.join("Google/Chrome/User Data/Default/Code Cache"));
            }
            if lower.contains("msedge") {
                dirs.push(base.join("Microsoft/Edge/User Data/Default/Cache"));
                dirs.push(base.join("Microsoft/Edge/User Data/Default/Code Cache"));
            }
        }
    }

    if dirs.is_empty() {
        None
    } else {
        Some(dirs)
    }
}
