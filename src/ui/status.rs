use std::time::{SystemTime, UNIX_EPOCH};

use byte_unit::{Byte, UnitType};

use crate::domain::models::{AppUsage, SystemSnapshot};
use crate::services::usage_service::UsageMap;

pub type UsageLookup<'a> = Option<&'a UsageMap>;

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

pub fn print_status(snap: &SystemSnapshot, usage: UsageLookup) -> anyhow::Result<()> {
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
    match usage {
        Some(map) => {
            println!("  {:>8}  {:<26} {:>10}  {}", "PID", "NAME", "MEM", "LAST RUN");
            for p in &snap.top_processes {
                let last = map
                    .get(&p.name.to_lowercase())
                    .map_or("unknown".to_string(), |u: &AppUsage| ago(u.last_run_unix, now));
                println!(
                    "  {:>8}  {:<26} {:>10}  {}",
                    p.pid,
                    truncate(&p.name, 26),
                    fmt(p.memory_bytes),
                    last
                );
            }
        }
        None => {
            println!("  {:>8}  {:<28} {}", "PID", "NAME", "MEM");
            for p in &snap.top_processes {
                println!(
                    "  {:>8}  {:<28} {}",
                    p.pid,
                    truncate(&p.name, 28),
                    fmt(p.memory_bytes)
                );
            }
        }
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
