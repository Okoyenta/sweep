use std::time::{SystemTime, UNIX_EPOCH};

use byte_unit::{Byte, UnitType};

use crate::domain::models::{ProcessMemInfo, SystemSnapshot};

/// Column header for the process table's last cell.
const PROCESS_HEADER: &str = "STARTED";

pub fn fmt(bytes: u64) -> String {
    let unit = Byte::from_u64(bytes).get_appropriate_unit(UnitType::Binary);
    format!("{:.2} {}", unit.get_value(), unit.get_unit())
}

fn pct(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) * 100.0
    }
}

fn bar(used: u64, total: u64, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        ((used.min(total) as f64 / total as f64) * width as f64).round() as usize
    };
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        if i < filled {
            s.push('#');
        } else {
            s.push('-');
        }
    }
    s.push(']');
    s
}

fn ago(unix: i64, now: i64) -> String {
    let d = (now - unix).max(0);
    if d < 60 {
        "just now".to_string()
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else if d < 86400 {
        format!("{}h ago", d / 3600)
    } else {
        format!("{}d ago", d / 86400)
    }
}

/// One row of the process table.
///
/// The last cell is the process's own start time, not the exe's last-launch
/// time. The launch history lives in the usage probes, and unelevated those can
/// only see programs the Windows shell started — which excludes the browsers
/// and editors that make up most of a top-RAM table, so the column read
/// `unknown` for every row and looked like a fact about the machine.
fn process_row(p: &ProcessMemInfo, now: i64) -> String {
    format!(
        "  {:>8}  {:<26} {:>10}  {}",
        p.pid,
        truncate(&p.name, 26),
        fmt(p.memory_bytes),
        ago(p.start_unix as i64, now)
    )
}

pub fn print_status(snap: &SystemSnapshot) -> anyhow::Result<()> {
    let m = &snap.memory;
    println!("memory");
    println!(
        "  {} {} / {} used ({:.1}%), {} free",
        bar(m.used_bytes, m.total_bytes, 30),
        fmt(m.used_bytes),
        fmt(m.total_bytes),
        pct(m.used_bytes, m.total_bytes),
        fmt(m.available_bytes)
    );
    if m.swap_total_bytes > 0 {
        println!(
            "  swap {} {} / {} used ({:.1}%)",
            bar(m.swap_used_bytes, m.swap_total_bytes, 20),
            fmt(m.swap_used_bytes),
            fmt(m.swap_total_bytes),
            pct(m.swap_used_bytes, m.swap_total_bytes)
        );
    }

    println!("\ndisks");
    for d in &snap.disks {
        println!(
            "  {:<12} {} {} / {} used ({:.1}%), {} free",
            d.name,
            bar(d.used_bytes, d.total_bytes, 20),
            fmt(d.used_bytes),
            fmt(d.total_bytes),
            pct(d.used_bytes, d.total_bytes),
            fmt(d.available_bytes)
        );
    }

    println!("\ntop processes by ram");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    println!(
        "  {:>8}  {:<26} {:>10}  {}",
        "PID", "NAME", "MEM", PROCESS_HEADER
    );
    for p in &snap.top_processes {
        println!("{}", process_row(p, now));
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, mem: u64, start_unix: u64) -> ProcessMemInfo {
        ProcessMemInfo {
            pid,
            name: name.into(),
            memory_bytes: mem,
            read_bytes: 0,
            write_bytes: 0,
            total_written_bytes: 0,
            start_unix,
        }
    }

    /// The bug: every cell of the old `LAST RUN` column read `unknown`, because
    /// the usage probes cannot see shell-external launches. No process row may
    /// contain that word now — the start time is always known.
    #[test]
    fn every_process_row_reports_a_start_time_not_unknown() {
        let now = 1_800_000_000i64;
        for start in [now as u64 - 30, now as u64 - 7200, now as u64 - 3 * 86_400] {
            let row = process_row(&proc(20112, "chrome.exe", 159_600_000, start), now);
            assert!(!row.contains("unknown"), "got: {row}");
            assert!(row.contains("chrome.exe"), "got: {row}");
        }
    }

    #[test]
    fn process_row_picks_the_right_age_bucket() {
        let now = 1_800_000_000i64;
        let cell = |secs: i64| process_row(&proc(1, "x.exe", 1, (now - secs) as u64), now);
        // `ends_with` rather than a last-token compare: "just now" is two words.
        assert!(cell(30).ends_with("just now"), "got: {}", cell(30));
        assert!(cell(7200).ends_with("2h ago"), "got: {}", cell(7200));
        assert!(cell(3 * 86_400).ends_with("3d ago"), "got: {}", cell(3 * 86_400));
    }

    /// A clock skewed behind the process start must not underflow into a
    /// nonsensical age.
    #[test]
    fn a_future_start_time_reads_as_just_now() {
        let now = 1_800_000_000i64;
        let row = process_row(&proc(1, "x.exe", 1, now as u64 + 60_000), now);
        assert!(row.ends_with("just now"), "got: {row}");
    }
}
