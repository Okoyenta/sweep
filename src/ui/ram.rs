use crate::domain::models::TrimOutcome;
use crate::ui::status::fmt;

pub fn print_ram_report(
    before_used: u64,
    after_used: u64,
    total: u64,
    outcome: &TrimOutcome,
) {
    let delta = before_used.saturating_sub(after_used);
    println!(
        "ram: {} -> {} used of {} (freed ~{})",
        fmt(before_used),
        fmt(after_used),
        fmt(total),
        fmt(delta)
    );
    if !outcome.attempted_pids.is_empty() {
        println!(
            "working set trim: {} processes trimmed, {} failed (protected or elevated)",
            outcome.succeeded, outcome.failed
        );
    }
    if outcome.standby_attempted {
        if outcome.standby_ok {
            println!("standby list purged");
        } else {
            println!("standby purge failed");
        }
    }
}
