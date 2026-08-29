use crate::domain::models::SystemSnapshot;
use crate::domain::traits::SystemMonitor;

pub struct SystemService<M: SystemMonitor> {
    monitor: M,
}

impl<M: SystemMonitor> SystemService<M> {
    pub fn new(monitor: M) -> Self {
        Self { monitor }
    }

    pub fn status_report(&mut self, top_processes: usize) -> anyhow::Result<SystemSnapshot> {
        let mut snap = self.monitor.snapshot()?;
        snap.top_processes.truncate(top_processes);
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{MemoryStats, ProcessMemInfo};

    fn proc(pid: u32, name: &str, mem: u64) -> ProcessMemInfo {
        ProcessMemInfo {
            pid,
            name: name.into(),
            memory_bytes: mem,
            read_bytes: 0,
            write_bytes: 0,
            total_written_bytes: 0,
        }
    }

    struct MockMonitor;

    impl SystemMonitor for MockMonitor {
        fn snapshot(&mut self) -> anyhow::Result<SystemSnapshot> {
            Ok(SystemSnapshot {
                memory: MemoryStats {
                    total_bytes: 16_000_000_000,
                    used_bytes: 8_000_000_000,
                    available_bytes: 8_000_000_000,
                    swap_total_bytes: 0,
                    swap_used_bytes: 0,
                },
                disks: vec![],
                top_processes: vec![
                    proc(42, "chrome", 900),
                    proc(7, "code", 500),
                    proc(11, "mock", 100),
                ],
            })
        }
    }

    #[test]
    fn status_report_returns_monitor_data() {
        let mut svc = SystemService::new(MockMonitor);
        let snap = svc.status_report(10).unwrap();
        assert_eq!(snap.memory.total_bytes, 16_000_000_000);
        assert_eq!(snap.top_processes.len(), 3);
        assert!(snap.disks.is_empty());
    }

    #[test]
    fn status_report_truncates_to_requested_top() {
        let mut svc = SystemService::new(MockMonitor);
        let snap = svc.status_report(2).unwrap();
        assert_eq!(snap.top_processes.len(), 2);
        assert_eq!(snap.top_processes[0].name, "chrome");
        assert_eq!(snap.top_processes[1].name, "code");
    }
}
