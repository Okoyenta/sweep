use crate::domain::models::{MemoryStats, SystemSnapshot, TrimOutcome};
use crate::domain::traits::{RamTrimmer, SystemMonitor};

pub struct RamReport {
    pub before: MemoryStats,
    pub after: MemoryStats,
    pub outcome: TrimOutcome,
}

pub struct RamService<M: SystemMonitor, T: RamTrimmer> {
    monitor: M,
    trimmer: T,
}

impl<M: SystemMonitor, T: RamTrimmer> RamService<M, T> {
    pub fn new(monitor: M, trimmer: T) -> Self {
        Self { monitor, trimmer }
    }

    /// trims the working sets of the top-N memory consumers (monitor order),
    /// optionally purges the standby list, then re-measures
    pub fn optimize(
        &mut self,
        trim_top: Option<usize>,
        purge_standby: bool,
    ) -> anyhow::Result<RamReport> {
        let snap_before = self.monitor.snapshot()?;
        let mut outcome = TrimOutcome::default();

        if let Some(n) = trim_top {
            let pids: Vec<u32> = top_pids(&snap_before, n);
            outcome.attempted_pids = pids.clone();
            let (ok, bad) = self.trimmer.trim_processes(&pids)?;
            outcome.succeeded = ok;
            outcome.failed = bad;
        }

        if purge_standby {
            outcome.standby_attempted = true;
            outcome.standby_ok = self.trimmer.purge_standby()?;
        }

        let snap_after = self.monitor.snapshot()?;
        Ok(RamReport {
            before: snap_before.memory,
            after: snap_after.memory,
            outcome,
        })
    }
}

fn top_pids(snap: &SystemSnapshot, n: usize) -> Vec<u32> {
    snap.top_processes.iter().take(n).map(|p| p.pid).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{DiskStats, ProcessMemInfo};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct SharedState {
        used: Rc<RefCell<u64>>,
    }

    struct MockMonitor {
        state: SharedState,
    }

    impl SystemMonitor for MockMonitor {
        fn snapshot(&mut self) -> anyhow::Result<SystemSnapshot> {
            let used = *self.state.used.borrow();
            Ok(SystemSnapshot {
                memory: MemoryStats {
                    total_bytes: 1000,
                    used_bytes: used,
                    available_bytes: 1000 - used,
                    swap_total_bytes: 0,
                    swap_used_bytes: 0,
                },
                disks: vec![DiskStats {
                    name: "C".into(),
                    mount_point: std::path::PathBuf::from("C:\\"),
                    total_bytes: 10,
                    used_bytes: 5,
                    available_bytes: 5,
                }],
                top_processes: vec![
                    ProcessMemInfo { pid: 111, name: "big.exe".into(), memory_bytes: 500, read_bytes: 0, write_bytes: 0, total_written_bytes: 0 },
                    ProcessMemInfo { pid: 222, name: "mid.exe".into(), memory_bytes: 300, read_bytes: 0, write_bytes: 0, total_written_bytes: 0 },
                    ProcessMemInfo { pid: 333, name: "small.exe".into(), memory_bytes: 100, read_bytes: 0, write_bytes: 0, total_written_bytes: 0 },
                ],
            })
        }
    }

    struct RecorderTrimmer {
        calls: RefCell<Vec<Vec<u32>>>,
        standby_result: bool,
        state: SharedState,
    }

    impl RamTrimmer for RecorderTrimmer {
        fn trim_processes(&mut self, pids: &[u32]) -> anyhow::Result<(u32, u32)> {
            self.calls.borrow_mut().push(pids.to_vec());
            *self.state.used.borrow_mut() -= 200;
            Ok((pids.len() as u32 - 1, 1))
        }
        fn purge_standby(&mut self) -> anyhow::Result<bool> {
            Ok(self.standby_result)
        }
    }

    fn service(used: u64) -> RamService<MockMonitor, RecorderTrimmer> {
        let state = SharedState {
            used: Rc::new(RefCell::new(used)),
        };
        RamService::new(
            MockMonitor {
                state: state.clone(),
            },
            RecorderTrimmer {
                calls: RefCell::new(vec![]),
                standby_result: true,
                state,
            },
        )
    }

    #[test]
    fn trims_only_top_n_pids_in_monitor_order() {
        let mut svc = service(900);
        let report = svc.optimize(Some(2), false).unwrap();

        assert_eq!(report.outcome.attempted_pids, vec![111, 222]);
        assert_eq!(report.outcome.succeeded, 1);
        assert_eq!(report.outcome.failed, 1);
        assert!(!report.outcome.standby_attempted);
        assert!(report.after.used_bytes < report.before.used_bytes);
    }

    #[test]
    fn standby_flag_flows_into_outcome() {
        let mut svc = service(800);
        let report = svc.optimize(None, true).unwrap();
        assert!(report.outcome.standby_attempted && report.outcome.standby_ok);
        assert!(report.outcome.attempted_pids.is_empty());
    }
}
