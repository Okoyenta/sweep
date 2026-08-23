use std::io::Write;

use crate::domain::models::InstalledApp;
use crate::ui::status::fmt;

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

pub fn print_apps(apps: &[InstalledApp], now_unix: i64) {
    println!(
        "  {:<34} {:<14} {:>9}  {:<10} {}",
        "NAME", "VERSION", "SIZE", "LAST RUN", "PUBLISHER"
    );
    let mut total = 0u64;
    for a in apps {
        total += a.size_bytes.unwrap_or(0);
        let last = a
            .last_run_unix
            .map_or_else(|| "unknown".to_string(), |t| ago(t, now_unix));
        println!(
            "  {:<34} {:<14} {:>9}  {:<10} {}",
            truncate(&a.name, 34),
            truncate(&a.version, 14),
            a.size_bytes.map(fmt).unwrap_or_else(|| "-".into()),
            last,
            truncate(&a.publisher, 30),
        );
    }
    println!("\n{} apps, {} total installed size", apps.len(), fmt(total));
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

pub fn confirm(prompt: &str) -> bool {
    print!("{} [y/N] ", prompt);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}
