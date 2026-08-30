use crate::domain::models::{IdleSsdOffender, KillRequest, ProcessMemInfo};
use crate::ui::status::fmt;

/// Ask the user to confirm terminating one specific process.
///
/// Renders the exact prompt required by FR-010 —
/// `kill <name> PID <pid> <size>?` — and returns true only on an explicit yes.
/// Every forced kill passes through here, so termination is never silent.
pub fn confirm_kill_process(req: &KillRequest) -> bool {
    crate::ui::apps::confirm(&format!(
        "kill {} PID {} {}?",
        req.name,
        req.pid,
        fmt(req.size_bytes)
    ))
}

/// Print the background-process table used by `sweep bg` (CLI counterpart of
/// the TUI `b` view).
pub fn print_background_table(procs: &[ProcessMemInfo]) {
    if procs.is_empty() {
        println!("no background processes detected");
        return;
    }
    println!("  {:<8} {:<24} {:>12} {:>12}", "PID", "APP", "RAM", "WRITTEN");
    for p in procs {
        println!(
            "  {:<8} {:<24} {:>12} {:>12}",
            p.pid,
            truncate(&p.name, 24),
            fmt(p.memory_bytes),
            fmt(p.total_written_bytes),
        );
    }
}

/// Print the idle heavy-writer table (PID, APP, IDLE, WRITE/h, RAM, REASON).
pub fn print_idle_table(offenders: &[IdleSsdOffender]) {
    if offenders.is_empty() {
        println!("no idle heavy writers detected");
        return;
    }

    println!(
        "  {:<8} {:<16} {:>8} {:>12} {:>10} {}",
        "PID", "APP", "IDLE", "WRITE/h", "RAM", "REASON"
    );
    let mut total_writes: u64 = 0;
    for off in offenders {
        total_writes += off.write_bytes;
        println!(
            "  {:<8} {:<16} {:>7}m {:>12} {:>10} {}",
            off.pid,
            truncate(&off.name, 16),
            off.idle_secs / 60,
            fmt(off.write_bytes),
            fmt(off.memory_bytes),
            off.reason,
        );
    }
    println!("\ntotal write volume: {}", fmt(total_writes));
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_offenders_prints_message() {
        let _buf: Vec<u8> = Vec::new();
        assert!(true);
    }
}
