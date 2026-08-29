use crate::domain::models::{BenchmarkSample, CategoryScan, CleanOutcome, LockedProcess};
use crate::ui::status::fmt;

pub fn print_kill_list(apps: &[LockedProcess]) {
    if apps.is_empty() {
        return;
    }
    println!("  {:<8} {:<24} {}", "PID", "APP", "LOCKED PATH");
    let mut seen = std::collections::HashSet::new();
    for a in apps {
        let key = (a.pid, a.path.to_string_lossy().into_owned());
        if !seen.insert(key) {
            continue;
        }
        println!("  {:<8} {:<24} {}", a.pid, a.name, a.path.display());
    }
}

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

pub fn print_benchmark(sample: &BenchmarkSample) {
    println!(
        "before {} free -> after {} free (freed {}) in {:.1}s",
        fmt(sample.before_free_bytes),
        fmt(sample.after_free_bytes),
        fmt(sample.freed_bytes()),
        sample.elapsed_secs,
    );
    if !sample.category_bytes.is_empty() {
        for (id, bytes) in &sample.category_bytes {
            if *bytes > 0 {
                println!("  {}: {}", id, fmt(*bytes));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_freed_bytes_subtracts() {
        let s = BenchmarkSample {
            before_free_bytes: 1000,
            after_free_bytes: 3000,
            elapsed_secs: 1.5,
            category_bytes: vec![],
        };
        assert_eq!(s.freed_bytes(), 2000);
    }

    #[test]
    fn benchmark_freed_bytes_saturates_at_zero() {
        let s = BenchmarkSample {
            before_free_bytes: 5000,
            after_free_bytes: 3000,
            elapsed_secs: 0.5,
            category_bytes: vec![],
        };
        assert_eq!(s.freed_bytes(), 0);
    }

    #[test]
    fn print_benchmark_format_contains_key_parts() {
        let s = BenchmarkSample {
            before_free_bytes: 1024 * 1024,
            after_free_bytes: 2 * 1024 * 1024,
            elapsed_secs: 2.3,
            category_bytes: vec![],
        };
        let mut buf = Vec::new();
        use std::io::Write;
        writeln!(buf, "{}", format!(
            "before {} free -> after {} free (freed {}) in {:.1}s",
            fmt(s.before_free_bytes),
            fmt(s.after_free_bytes),
            fmt(s.freed_bytes()),
            s.elapsed_secs,
        )).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("before"));
        assert!(out.contains("after"));
        assert!(out.contains("freed"));
        assert!(out.contains("in 2.3s"));
    }
}
