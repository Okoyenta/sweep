use std::fs::OpenOptions;
use std::io::Write;

#[cfg(not(windows))]
use std::fs::File;

use crate::domain::models::{GuardLogLevel, GuardLogEntry};

pub struct GuardLog;

impl GuardLog {
    pub fn write(entry: &GuardLogEntry) -> anyhow::Result<()> {
        let path = crate::infra::paths::guard_log_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(
            file,
            "[{}] [{}] [{}]{}",
            entry.timestamp,
            entry.level,
            entry.message,
            entry
                .bytes_freed
                .map(|b| format!(" [{}]", b))
                .unwrap_or_default()
        )?;
        Ok(())
    }

    pub fn info(message: &str) -> anyhow::Result<()> {
        Self::write(&GuardLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: GuardLogLevel::Info,
            message: message.to_string(),
            bytes_freed: None,
        })
    }

    pub fn action(message: &str, bytes_freed: Option<u64>) -> anyhow::Result<()> {
        Self::write(&GuardLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: GuardLogLevel::Action,
            message: message.to_string(),
            bytes_freed,
        })
    }

    pub fn warn(message: &str) -> anyhow::Result<()> {
        Self::write(&GuardLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: GuardLogLevel::Warn,
            message: message.to_string(),
            bytes_freed: None,
        })
    }
}

#[cfg(windows)]
pub struct GuardLock {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl GuardLock {
    pub fn acquire() -> anyhow::Result<Self> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name: Vec<u16> = OsStr::new("Global\\SweepGuard")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if handle == std::ptr::null_mut() {
            anyhow::bail!("failed to create guard mutex");
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            anyhow::bail!("guard is already running (mutex held by another instance)");
        }
        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for GuardLock {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(not(windows))]
pub struct GuardLock {
    _lock_file: File,
}

#[cfg(not(windows))]
impl GuardLock {
    pub fn acquire() -> anyhow::Result<Self> {
        let path = crate::infra::paths::guard_lock_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self { _lock_file: file })
    }
}

#[cfg(not(windows))]
impl Drop for GuardLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(crate::infra::paths::guard_lock_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_log_writes_entries() {
        let path = crate::infra::paths::guard_log_path();
        let _ = std::fs::remove_file(&path);
        GuardLog::info("test message").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("INFO"));
        assert!(content.contains("test message"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn guard_lock_acquire_and_drop() {
        let lock = GuardLock::acquire().unwrap();
        drop(lock);
        let _lock2 = GuardLock::acquire().unwrap();
    }
}

/// Minimum sustained idle time before guard may gracefully close a process
/// under `--allow-kill` (FR-012: more than 60 minutes).
const GUARD_CLOSE_MIN_IDLE_MINS: u64 = 60;

/// Minimum write rate before guard may gracefully close a process under
/// `--allow-kill` (FR-012: more than 500 MB/h).
const GUARD_CLOSE_MIN_WRITE_BYTES_PER_HOUR: f64 = 500.0 * 1024.0 * 1024.0;

pub struct GuardService<M: crate::domain::traits::GuardMonitor> {
    monitor: M,
    config: crate::domain::models::GuardConfig,
    ram_pressure_count: usize,
    cooldown_until: Option<std::time::Instant>,
}

impl<M: crate::domain::traits::GuardMonitor> GuardService<M> {
    pub fn new(monitor: M, config: crate::domain::models::GuardConfig) -> Self {
        Self {
            monitor,
            config,
            ram_pressure_count: 0,
            cooldown_until: None,
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        if self.config.once {
            self.poll_cycle()?;
        } else {
            loop {
                self.poll_cycle()?;
                std::thread::sleep(std::time::Duration::from_secs(self.config.interval_secs));
            }
        }
        Ok(())
    }

    fn poll_cycle(&mut self) -> anyhow::Result<()> {
        use crate::domain::models::GuardAction;

        let ram = self.monitor.snapshot_ram()?;
        let disk = self.monitor.snapshot_disk()?;
        let mut action = GuardAction::None;
        let mut freed: Option<u64> = None;

        if let Some(cooldown) = self.cooldown_until {
            if std::time::Instant::now() < cooldown {
                crate::ui::guard::print_guard_idle();
                return Ok(());
            }
            self.cooldown_until = None;
        }

        if ram.used_pct >= self.config.ram_threshold {
            self.ram_pressure_count += 1;
            if self.ram_pressure_count >= crate::domain::models::GUARD_HYSTERESIS_SAMPLES {
                let bytes_freed = self.handle_ram_pressure()?;
                action = GuardAction::RamTrim;
                freed = Some(bytes_freed);
                self.cooldown_until = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_secs(crate::domain::models::GUARD_COOLDOWN_SECS),
                );
                GuardLog::action(
                    &format!("ram trim: freed {} bytes", bytes_freed),
                    Some(bytes_freed),
                )?;
                crate::ui::guard::send_toast(
                    "Sweep Guard",
                    &format!("RAM trimmed, freed ~{} ", crate::ui::status::fmt(bytes_freed)),
                )?;
                self.ram_pressure_count = 0;
            } else {
                GuardLog::info(&format!(
                    "ram pressure {}% (sample {}/{})",
                    ram.used_pct * 100.0,
                    self.ram_pressure_count,
                    crate::domain::models::GUARD_HYSTERESIS_SAMPLES,
                ))?;
            }
        } else {
            self.ram_pressure_count = 0;
        }

        let disk_min_bytes = self.config.disk_min_gb * 1024 * 1024 * 1024;
        if disk.free_bytes < disk_min_bytes {
            let bytes_freed = self.handle_disk_rescue(disk.free_bytes, disk_min_bytes)?;
            action = match bytes_freed {
                b if b > 0 => {
                    GuardLog::action(
                        &format!("disk rescue: freed {} bytes", bytes_freed),
                        Some(bytes_freed),
                    )?;
                    crate::ui::guard::send_toast(
                        "Sweep Guard",
                        &format!("Disk rescue freed ~{}", crate::ui::status::fmt(bytes_freed)),
                    )?;
                    self.cooldown_until = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_secs(crate::domain::models::GUARD_COOLDOWN_SECS),
                    );
                    GuardAction::DiskCleanSafe
                }
                _ => GuardAction::None,
            };
            freed = Some(bytes_freed);
        }

        // Tier 2 escalation, opt-in only. Without --allow-kill guard is strictly
        // trim-only and never touches a process (FR-013, Principle II).
        if self.config.allow_kill {
            self.close_idle_offenders()?;
        }

        let disk_free_gb = disk.free_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let ram_pct = ram.used_pct;
        crate::ui::guard::print_guard_cycle(ram_pct, disk_free_gb, &action.to_string(), freed);

        Ok(())
    }

    /// Gracefully close idle offenders sustaining heavy writes, when the user
    /// passed `--allow-kill`.
    ///
    /// Guard only ever performs a tier-2 graceful close — never a forced kill —
    /// and only for processes exceeding both the write-rate and idle-duration
    /// thresholds (FR-012). Every decision, including blocklist skips, is
    /// written to the audit log.
    fn close_idle_offenders(&mut self) -> anyhow::Result<()> {
        use crate::domain::models::{KillMode, KillRequest};
        use crate::services::idle_service::{IdleConfig, IdleService};
        use crate::services::kill_service::KillService;

        let config = IdleConfig {
            top: usize::MAX,
            idle_mins: GUARD_CLOSE_MIN_IDLE_MINS,
            min_write_mb: 0,
            clean_cache: false,
        };
        let offenders = match IdleService::new().detect_fast(&config) {
            Ok(o) => o,
            Err(e) => {
                GuardLog::warn(&format!("idle detection failed: {e}"))?;
                return Ok(());
            }
        };

        let svc = KillService::new();
        for off in offenders {
            if off.writes_per_hour < GUARD_CLOSE_MIN_WRITE_BYTES_PER_HOUR {
                continue;
            }
            let req = KillRequest {
                pid: off.pid,
                name: off.name.clone(),
                size_bytes: off.memory_bytes,
                mode: KillMode::Close,
                // The user consented to tier-2 closes by passing --allow-kill.
                consent: true,
            };
            if KillService::is_blocked(&req) {
                GuardLog::info(&format!(
                    "skipped {} (PID {}): protected system process",
                    req.name, req.pid
                ))?;
                continue;
            }
            if svc.execute(&req) {
                GuardLog::action(
                    &format!(
                        "graceful close (allow-kill consent): {} PID {} writing {:.0} MB/h",
                        req.name,
                        req.pid,
                        off.writes_per_hour / (1024.0 * 1024.0)
                    ),
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn handle_ram_pressure(&mut self) -> anyhow::Result<u64> {
        #[cfg(windows)]
        {
            use crate::infra::sysinfo_monitor::SysinfoMonitor;
            use crate::infra::win::ram::WinRamTrimmer;
            use crate::services::ram_service::RamService;

            let mut svc = RamService::new(SysinfoMonitor::new(), WinRamTrimmer::new());
            let report = svc.optimize(Some(10), true)?;
            let freed = report.before.used_bytes.saturating_sub(report.after.used_bytes);
            Ok(freed)
        }
        #[cfg(not(windows))]
        {
            let _ = (self);
            Ok(0)
        }
    }

    fn handle_disk_rescue(&mut self, free_bytes: u64, target_bytes: u64) -> anyhow::Result<u64> {
        let _deficit = target_bytes.saturating_sub(free_bytes);
        let mut total_freed = 0u64;

        {
            let freed = crate::infra::paths::consume_reserve();
            if let Some(bytes) = freed {
                total_freed += bytes;
                if free_bytes + total_freed >= target_bytes {
                    return Ok(total_freed);
                }
            }
        }

        {
            use crate::domain::models::RiskLevel;
            use crate::services::clean_service::{discover_with_policy, CleanService};

            // Unattended cleaning must honor the user's `sweep.toml` exclusions
            // just like an interactive `sweep clean` does (FR-005).
            let discovered = discover_with_policy(None, None, false);
            if discovered.excluded > 0 {
                GuardLog::info(&format!(
                    "disk rescue: {} category/categories excluded by sweep.toml",
                    discovered.excluded
                ))?;
            }

            let safe_cats: Vec<_> = discovered
                .categories
                .into_iter()
                .filter(|c| c.risk == RiskLevel::Safe)
                .collect();

            let svc = CleanService::new(crate::infra::trash_remover::TrashRemover::new());
            let scans = svc.scan_excluding(&safe_cats, &discovered.exclusions);
            let outcome = svc.run(&scans, None)?;
            // Guard-trashed items are recoverable with `sweep undo` too.
            if let Err(e) = crate::infra::undo::append_session(outcome.undo_items.clone()) {
                GuardLog::warn(&format!("could not write undo journal: {e}"))?;
            }
            total_freed += outcome.removed_bytes;
            if free_bytes + total_freed >= target_bytes {
                return Ok(total_freed);
            }
        }

        {
            use crate::domain::traits::RecycleBin;
            let bin = crate::infra::trash_remover::TrashBin::new();
            let count = bin.purge_all()?;
            GuardLog::action(&format!("purged {count} recycle bin items"), None)?;
            total_freed += count * 1024;
        }

        Ok(total_freed)
    }
}
