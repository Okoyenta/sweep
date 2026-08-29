use sysinfo::{Disks, ProcessesToUpdate, System};

use crate::domain::models::{DiskStats, MemoryStats, ProcessMemInfo, SystemSnapshot};
use crate::domain::traits::{GuardMonitor, SystemMonitor};

const TOP_PROCESSES: usize = 10;

pub struct SysinfoMonitor {
    system: System,
    disks: Disks,
}

impl SysinfoMonitor {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new(),
        }
    }
}

impl Default for SysinfoMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn disk_label(name: &std::ffi::OsStr, mount_point: &std::path::Path) -> String {
    let n = name.to_string_lossy();
    if n.trim().is_empty() {
        mount_point.display().to_string()
    } else {
        n.into_owned()
    }
}

impl SystemMonitor for SysinfoMonitor {
    fn snapshot(&mut self) -> anyhow::Result<SystemSnapshot> {
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh(true);

        let memory = MemoryStats {
            total_bytes: self.system.total_memory(),
            used_bytes: self.system.used_memory(),
            available_bytes: self.system.available_memory(),
            swap_total_bytes: self.system.total_swap(),
            swap_used_bytes: self.system.used_swap(),
        };

        let disks: Vec<DiskStats> = self
            .disks
            .list()
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| {
                let total = d.total_space();
                let available = d.available_space();
                DiskStats {
                    name: disk_label(d.name(), d.mount_point()),
                    mount_point: d.mount_point().to_path_buf(),
                    total_bytes: total,
                    used_bytes: total.saturating_sub(available),
                    available_bytes: available,
                }
            })
            .collect();

        let mut processes: Vec<ProcessMemInfo> = self
            .system
            .processes()
            .iter()
            .map(|(pid, proc)| {
                let disk = proc.disk_usage();
                ProcessMemInfo {
                    pid: pid.as_u32(),
                    name: proc.name().to_string_lossy().into_owned(),
                    memory_bytes: proc.memory(),
                    read_bytes: disk.read_bytes,
                    write_bytes: disk.written_bytes,
                    total_written_bytes: disk.total_written_bytes,
                }
            })
            .collect();
        processes.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes).then(a.pid.cmp(&b.pid)));
        processes.truncate(TOP_PROCESSES);

        Ok(SystemSnapshot {
            memory,
            disks,
            top_processes: processes,
        })
    }
}

impl GuardMonitor for SysinfoMonitor {
    fn snapshot_ram(&self) -> anyhow::Result<crate::domain::models::RamSnapshot> {
        let used = self.system.used_memory();
        let total = self.system.total_memory();
        let available = self.system.available_memory();
        let used_pct = if total > 0 {
            used as f64 / total as f64
        } else {
            0.0
        };
        Ok(crate::domain::models::RamSnapshot {
            timestamp_secs: chrono::Utc::now().timestamp(),
            used_bytes: used,
            total_bytes: total,
            available_bytes: available,
            used_pct,
        })
    }

    fn snapshot_disk(&self) -> anyhow::Result<crate::domain::models::DiskSnapshot> {
        let free = crate::infra::paths::free_bytes_on_index_volume();
        let total = self
            .disks
            .list()
            .iter()
            .map(|d| d.total_space())
            .max()
            .unwrap_or(0);
        Ok(crate::domain::models::DiskSnapshot {
            timestamp_secs: chrono::Utc::now().timestamp(),
            free_bytes: free,
            total_bytes: total,
        })
    }
}
