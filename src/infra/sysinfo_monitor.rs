use sysinfo::{Disks, ProcessesToUpdate, System};

use crate::domain::models::{DiskStats, MemoryStats, ProcessMemInfo, SystemSnapshot};
use crate::domain::traits::SystemMonitor;

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
            .map(|(pid, proc)| ProcessMemInfo {
                pid: pid.as_u32(),
                name: proc.name().to_string_lossy().into_owned(),
                memory_bytes: proc.memory(),
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
