use crate::domain::models::{CategoryScan, CleanOutcome};
use crate::ui::status::fmt;

pub fn print_scans(scans: &[CategoryScan]) {
    if scans.is_empty() {
        println!("no cleanable categories found");
        return;
    }
    println!(
        "  {:<20} {:>10} {:>9}  {}",
        "CATEGORY", "SIZE", "FILES", "TITLE"
    );
    let mut total = 0u64;
    for s in scans {
        total += s.total_bytes;
        println!(
            "  {:<20} {:>10} {:>9}  {}",
            s.category_id,
            fmt(s.total_bytes),
            s.files,
            s.title
        );
    }
    println!("\ncleanable: {}", fmt(total));
}

pub fn print_outcome(outcome: &CleanOutcome, dry_run: bool) {
    if dry_run {
        return;
    }
    println!(
        "removed {} items ({}), {} failed (locked or protected; left in place)",
        outcome.removed_items,
        fmt(outcome.removed_bytes),
        outcome.failed_items
    );
}
