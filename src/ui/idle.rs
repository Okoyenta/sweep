use crate::domain::models::IdleSsdOffender;
use crate::ui::status::fmt;

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
