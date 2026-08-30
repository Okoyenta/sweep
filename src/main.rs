use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use sweep::domain::models::BenchmarkSample;
use sweep::domain::traits::{IndexStore, UsageProbe};
use sweep::infra::paths::{
    index_db_path, ensure_reserve, consume_reserve, try_recreate_reserve,
    free_bytes_on_index_volume, ensure_headroom_or_consume_reserve, is_disk_full_error,
};
use sweep::infra::sqlite_store::SqliteStore;
use sweep::services::index_service::{IndexConfig, IndexService};
use sweep::services::system_service::SystemService;
use sweep::ui::cli::{Cli, Command};
use sweep::ui::status::fmt;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = cli.config.clone();
    let rules = cli.rules.clone();
    let config = config.as_deref();
    let rules = rules.as_deref();
    if cli.version {
        return run_version();
    }
    match cli.command {
        Some(Command::Status { top }) => run_status(top),
        Some(Command::Index {
            status,
            full,
            roots,
        }) => run_index(status, full, roots),
        Some(Command::Apps {
            since_days,
            uninstall,
        }) => run_apps(since_days, uninstall.as_deref()),
        Some(Command::Clean {
            scan_only,
            only,
            yes,
            deep,
            stop_services,
            kill,
        }) => run_clean(scan_only, only, yes, deep, stop_services, kill, config, rules),
        Some(Command::Ram {
            trim_top,
            purge_standby,
        }) => run_ram(trim_top, purge_standby),
        Some(Command::Tui { top }) => run_tui(top),
        Some(Command::Bin { empty, yes }) => run_bin(empty, yes),
        Some(Command::Dupes {
            min_mb,
            trash_group,
            yes,
        }) => run_dupes(min_mb, trash_group, yes),
        Some(Command::Diagnose { deep }) => run_diagnose(deep, config, rules),
        Some(Command::Schedule {
            install,
            remove,
            status,
            guard_install,
            guard_remove,
            guard_status,
        }) => run_schedule(install, remove, status, guard_install, guard_remove, guard_status),
        Some(Command::Guard {
            ram_threshold,
            disk_min_gb,
            interval_secs,
            once,
            allow_service_stop,
            allow_kill,
        }) => run_guard(ram_threshold, disk_min_gb, interval_secs, once, allow_service_stop, allow_kill),
        Some(Command::Idle {
            top,
            idle_mins,
            min_write_mb,
            clean_cache,
            close,
            kill,
            force,
            only,
        }) => run_idle(
            top,
            idle_mins,
            min_write_mb,
            clean_cache,
            close,
            kill,
            force,
            &only,
        ),
        Some(Command::Doctor) => run_doctor(config, rules),
        Some(Command::Optimize {
            volume,
            analyze,
            yes,
        }) => run_optimize(volume.as_deref(), analyze, yes),
        Some(Command::Undo) => run_undo(),
        Some(Command::Bg {
            top,
            kill,
            force,
            only,
        }) => run_bg(top, kill, force, &only),
        None => run_status(10),
    }
}

/// `sweep --version` — print the version, plus an update hint when a newer
/// release is published. Offline or timed-out checks print the version only.
fn run_version() -> Result<()> {
    use sweep::services::system_service::{check_for_update, UpdateCheck};

    let current = env!("CARGO_PKG_VERSION");
    print!("sweep {current}");
    match check_for_update(current) {
        UpdateCheck::Available(tag) => println!("  update available: {tag}"),
        UpdateCheck::UpToDate => println!(),
        UpdateCheck::Skipped => println!("  (update check skipped)"),
    }
    Ok(())
}

/// `sweep doctor` — print the pre-flight safety report. Always exits 0.
fn run_doctor(
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> Result<()> {
    use sweep::services::doctor_service::DoctorService;

    let report = DoctorService::new().report(config, rules);
    sweep::ui::doctor::print_report(&report);
    Ok(())
}

/// `sweep optimize` — TRIM solid-state volumes, defragment rotational ones.
///
/// With no `--volume` it lists every volume and its detected media, changing
/// nothing. Maintenance is confirmed per volume unless `-y` is passed, because
/// a defrag pass is long-running and I/O-heavy.
fn run_optimize(volume: Option<&str>, analyze: bool, yes: bool) -> Result<()> {
    use sweep::services::optimize_service::{action_description, OptimizeService};

    let svc = OptimizeService::new();
    let volumes = svc.volumes();

    let Some(target) = volume else {
        sweep::ui::optimize::print_volumes(&volumes);
        println!("\npass --volume <mount> to maintain one (add --analyze to preview)");
        return Ok(());
    };

    let want = normalize_mount(target);
    let Some(info) = volumes
        .iter()
        .find(|v| normalize_mount(&v.mount) == want)
    else {
        anyhow::bail!(
            "no volume matching '{target}'; run `sweep optimize` to list detected volumes"
        );
    };

    let action = sweep::services::optimize_service::action_for(info.media);
    println!(
        "{} [{}]: {}",
        info.mount,
        info.media,
        action_description(&action, &info.mount)
    );

    // Both Optimize-Volume and fstrim require elevation; say so up front rather
    // than letting the tool fail with a stack trace after the user consents.
    if !is_elevated() {
        println!(
            "note: not elevated — drive maintenance needs {}",
            if cfg!(windows) {
                "an Administrator prompt"
            } else {
                "root (sudo)"
            }
        );
    }

    if !analyze {
        let proceed = yes
            || sweep::ui::apps::confirm(&format!("run {action} on {}?", info.mount));
        if !proceed {
            println!("aborted");
            return Ok(());
        }
    }

    let outcome = svc.run(info, analyze);
    sweep::ui::optimize::print_outcome(&outcome, analyze);
    Ok(())
}

/// Whether sweep currently holds administrator / root rights.
fn is_elevated() -> bool {
    use sweep::domain::models::ElevationStatus;

    #[cfg(windows)]
    let status = sweep::infra::win::doctor::elevation_status();
    #[cfg(not(windows))]
    let status = sweep::infra::linux::doctor::elevation_status();

    status == ElevationStatus::Elevated
}

/// Compare mount points case-insensitively, ignoring a trailing separator, so
/// `C:`, `C:\`, and `c:/` all select the same volume.
fn normalize_mount(mount: &str) -> String {
    let trimmed = mount.trim_end_matches(['\\', '/']);
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        // A bare "/" trims to empty; keep it addressable.
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            trimmed.to_string()
        }
    }
}

/// `sweep undo` — restore the newest trashed session. Exits 0 even when the
/// journal is empty or the Recycle Bin was purged.
fn run_undo() -> Result<()> {
    use sweep::services::undo_service::UndoService;

    let outcome = UndoService::new().undo();
    sweep::ui::undo::print_outcome(&outcome);
    Ok(())
}

fn open_store() -> Result<SqliteStore> {
    SqliteStore::open(&index_db_path()).context("opening index database")
}

fn open_store_with_reserve() -> Option<SqliteStore> {
    match SqliteStore::open(&index_db_path()) {
        Ok(store) => Some(store),
        Err(err) => {
            if is_disk_full_error(&err) {
                eprintln!("disk full error — consuming reserve file...");
                consume_reserve();
                match SqliteStore::open(&index_db_path()) {
                    Ok(store) => Some(store),
                    Err(_) => None,
                }
            } else {
                None
            }
        }
    }
}

#[cfg(windows)]
fn usage_probes() -> Vec<Box<dyn UsageProbe>> {
    vec![
        Box::new(sweep::infra::win::prefetch::PrefetchProbe::new()),
        Box::new(sweep::infra::win::userassist::UserAssistProbe::new()),
    ]
}

#[cfg(not(windows))]
fn usage_probes() -> Vec<Box<dyn UsageProbe>> {
    vec![]
}

#[cfg(windows)]
fn run_apps(since_days: Option<u64>, uninstall: Option<&str>) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    use sweep::services::app_service::AppService;
    use sweep::services::usage_service::UsageService;

    let inventory = sweep::infra::win::apps::RegistryAppInventory::new();
    let svc = AppService::new(inventory);
    let mut apps = svc.list().context("reading installed apps")?;

    let usage_service = UsageService::new(usage_probes());
    let usage_map = usage_service.collect_map();
    svc.attach_usage(&mut apps, &usage_map);

    if let Some(query) = uninstall {
        return uninstall_flow(&svc, &apps, query);
    }

    if let Some(days) = since_days {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        apps = svc.filter_unused_since(apps, days, now);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sweep::ui::apps::print_apps(&apps, now);
    Ok(())
}

#[cfg(windows)]
fn uninstall_flow(
    svc: &sweep::services::app_service::AppService<sweep::infra::win::apps::RegistryAppInventory>,
    apps: &[sweep::domain::models::InstalledApp],
    query: &str,
) -> Result<()> {
    let hits = svc.find(apps, query);
    match hits.len() {
        0 => anyhow::bail!("no installed app matches '{}'", query),
        1 => {}
        n => {
            println!("'{}' is ambiguous, {} matches:", query, n);
            for a in &hits {
                println!("  - {}", a.name);
            }
            anyhow::bail!("be more specific");
        }
    }
    let app = hits[0];
    let Some(cmd) = app.uninstall_command.as_ref() else {
        anyhow::bail!("'{}' exposes no uninstall command", app.name);
    };
    if !sweep::ui::apps::confirm(&format!(
        "run official uninstaller for '{}' ({})?",
        app.name,
        app.version
    )) {
        println!("aborted");
        return Ok(());
    }
    println!("launching: {}", cmd);
    std::process::Command::new("cmd")
        .args(["/C", cmd])
        .spawn()
        .context("spawning uninstaller")?;
    Ok(())
}

#[cfg(not(windows))]
fn run_apps(since_days: Option<u64>, uninstall: Option<&str>) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    use sweep::domain::traits::AppInventory;
    use sweep::infra::linux::apps::DesktopFileInventory;
    use sweep::services::app_service::AppService;

    let svc = AppService::new(DesktopFileInventory::new());
    let mut apps = svc.list().context("reading installed apps")?;

    if let Some(query) = uninstall {
        let hits = svc.find(&apps, query);
        match hits.len() {
            0 => anyhow::bail!("no installed app matches '{}'", query),
            1 => {
                let app = hits[0];
                if !sweep::ui::apps::confirm(&format!(
                    "uninstall '{}' ({})? (no official uninstaller detected; remove via your package manager)",
                    app.name, app.version
                )) {
                    println!("aborted");
                    return Ok(());
                }
                println!("remove it manually, e.g.: sudo apt remove <pkg> / sudo pacman -R <pkg>");
            }
            n => {
                println!("'{}' is ambiguous, {} matches:", query, n);
                for a in &hits {
                    println!("  - {}", a.name);
                }
                anyhow::bail!("be more specific");
            }
        }
        return Ok(());
    }

    if let Some(days) = since_days {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        apps = svc.filter_unused_since(apps, days, now);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sweep::ui::apps::print_apps(&apps, now);
    Ok(())
}

#[cfg(windows)]
fn run_clean(
    scan_only: bool,
    only: Vec<String>,
    yes: bool,
    deep: bool,
    stop_services: bool,
    kill: bool,
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> Result<()> {
    use sweep::services::clean_service::{discover_with_policy, CleanService};

    let _service_guard = if deep && stop_services {
        Some(sweep::infra::win::service_lock::ServiceGuard::new(&[
            "wuauserv", "bits",
        ])?)
    } else {
        None
    };

    let discovered = discover_with_policy(config, rules, deep);
    sweep::ui::clean::print_excluded(discovered.excluded);
    // When --only is set, only scan the requested categories to keep --scan-only fast
    // (avoids walking large stores like pnpm when not needed).
    let categories: Vec<_> = if only.is_empty() {
        discovered.categories
    } else {
        discovered
            .categories
            .into_iter()
            .filter(|c| only.iter().any(|id| id == &c.id))
            .collect()
    };
    let svc = CleanService::new(sweep::infra::trash_remover::TrashRemover::new());
    let scans = svc.scan_excluding(&categories, &discovered.exclusions);

    let selected = if only.is_empty() {
        scans.clone()
    } else {
        scans
            .iter()
            .filter(|s| only.iter().any(|id| id == &s.category_id))
            .cloned()
            .collect::<Vec<_>>()
    };

    if selected.is_empty() {
        println!("nothing to clean for the given filters");
        return Ok(());
    }

    sweep::ui::clean::print_scans(&selected);

    if scan_only {
        return Ok(());
    }

    let dry_run_total: u64 = selected.iter().map(|s| s.total_bytes).sum();
    let proceed = yes
        || sweep::ui::apps::confirm(&format!(
            "move these items to the recycle bin ({} freed estimate)?",
            sweep::ui::status::fmt(dry_run_total)
        ));
    if !proceed {
        println!("aborted");
        return Ok(());
    }

    ensure_headroom_or_consume_reserve();
    let before = free_bytes_on_index_volume();
    let start = Instant::now();
    let mut outcome = svc.run(&scans, Some(&only))?;

    // Kill-and-retry: if both --kill and locked files exist, ask, kill, retry.
    if kill && !outcome.failed_paths.is_empty() {
        let failed_refs: Vec<&std::path::Path> =
            outcome.failed_paths.iter().map(|p| p.as_path()).collect();
        let apps = sweep::infra::win::process_lock::locked_processes(&failed_refs);
        if !apps.is_empty() {
            sweep::ui::clean::print_kill_list(&apps);
            let approved = yes || sweep::ui::guard::confirm_kill(&apps);
            if approved {
                kill_processes(&apps);
                let retry_start = Instant::now();
                let retry = svc.run(&scans, Some(&only))?;
                // the retry replaces the outcome so the report reflects reality,
                // but both passes trashed items that undo must be able to restore
                let mut merged = retry;
                merged.undo_items.extend(outcome.undo_items);
                outcome = merged;
                record_undo_session(&outcome);
                let after = free_bytes_on_index_volume();
                sweep::ui::clean::print_outcome(&outcome, false);
                sweep::ui::clean::print_benchmark(&BenchmarkSample {
                    before_free_bytes: before,
                    after_free_bytes: after,
                    elapsed_secs: retry_start.elapsed().as_secs_f64(),
                    category_bytes: vec![],
                });
                try_recreate_reserve();
                return Ok(());
            } else {
                println!("skipped killing apps; locked files left in place");
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let after = free_bytes_on_index_volume();
    record_undo_session(&outcome);
    sweep::ui::clean::print_outcome(&outcome, false);
    sweep::ui::clean::print_benchmark(&BenchmarkSample {
        before_free_bytes: before,
        after_free_bytes: after,
        elapsed_secs: elapsed,
        category_bytes: vec![],
    });
    try_recreate_reserve();
    Ok(())
}

/// Append the items trashed by this run to the undo journal (FR-006).
///
/// Journal failures are reported but never fail the clean itself — the files are
/// already safely in the Recycle Bin.
fn record_undo_session(outcome: &sweep::domain::models::CleanOutcome) {
    if outcome.undo_items.is_empty() {
        return;
    }
    if let Err(e) = sweep::infra::undo::append_session(outcome.undo_items.clone()) {
        eprintln!("warning: could not write undo journal: {e}");
    } else {
        println!(
            "undo: {} item(s) recorded — restore with `sweep undo`",
            outcome.undo_items.len()
        );
    }
}

#[cfg(windows)]
fn kill_processes(apps: &[sweep::domain::models::LockedProcess]) {
    let mut pids: Vec<u32> = Vec::new();
    let own = std::process::id();
    for a in apps {
        if a.pid == own || pids.contains(&a.pid) {
            continue;
        }
        pids.push(a.pid);
        println!("  killing {} (PID {})", a.name, a.pid);
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &a.pid.to_string()])
            .output();
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
}

#[cfg(not(windows))]
fn run_clean(
    scan_only: bool,
    only: Vec<String>,
    yes: bool,
    deep: bool,
    _stop_services: bool,
    kill: bool,
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> Result<()> {
    use sweep::services::clean_service::{discover_with_policy, CleanService};

    let discovered = discover_with_policy(config, rules, deep);
    sweep::ui::clean::print_excluded(discovered.excluded);
    let categories: Vec<_> = if only.is_empty() {
        discovered.categories
    } else {
        discovered
            .categories
            .into_iter()
            .filter(|c| only.iter().any(|id| id == &c.id))
            .collect()
    };
    let svc = CleanService::new(sweep::infra::trash_remover::TrashRemover::new());
    let scans = svc.scan_excluding(&categories, &discovered.exclusions);

    let selected: Vec<_> = if only.is_empty() {
        scans.clone()
    } else {
        scans
            .iter()
            .filter(|s| only.iter().any(|id| id == &s.category_id))
            .cloned()
            .collect()
    };
    if selected.is_empty() {
        println!("nothing to clean for the given filters");
        return Ok(());
    }

    sweep::ui::clean::print_scans(&selected);
    if scan_only {
        return Ok(());
    }
    let total: u64 = selected.iter().map(|s| s.total_bytes).sum();
    if !(yes || sweep::ui::apps::confirm(&format!(
        "move these items to the trash ({})?",
        sweep::ui::status::fmt(total)
    ))) {
        println!("aborted");
        return Ok(());
    }
    ensure_headroom_or_consume_reserve();
    let before = free_bytes_on_index_volume();
    let start = Instant::now();
    let mut outcome = svc.run(&scans, Some(&only))?;

    if kill && !outcome.failed_paths.is_empty() {
        // group failed paths by category for the name heuristic
        let mut any_killed = false;
        for scan in &selected {
            let failed_in_cat: Vec<&std::path::PathBuf> = outcome
                .failed_paths
                .iter()
                .filter(|p| scan.items.contains(p))
                .collect();
            if failed_in_cat.is_empty() {
                continue;
            }
            let apps =
                sweep::infra::linux::process_lock::locked_processes_for_category(&scan.category_id, &failed_in_cat);
            if !apps.is_empty() {
                sweep::ui::clean::print_kill_list(&apps);
                let approved = yes || sweep::ui::guard::confirm_kill(&apps);
                if approved {
                    kill_processes(&apps);
                    any_killed = true;
                } else {
                    println!("skipped killing apps; locked files left in place");
                }
            }
        }
        if any_killed {
            let after = free_bytes_on_index_volume();
            let retry = svc.run(&scans, Some(&only))?;
            let mut merged = retry;
            merged.undo_items.extend(outcome.undo_items);
            outcome = merged;
            record_undo_session(&outcome);
            sweep::ui::clean::print_outcome(&outcome, false);
            sweep::ui::clean::print_benchmark(&BenchmarkSample {
                before_free_bytes: before,
                after_free_bytes: after,
                elapsed_secs: start.elapsed().as_secs_f64(),
                category_bytes: vec![],
            });
            try_recreate_reserve();
            return Ok(());
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let after = free_bytes_on_index_volume();
    record_undo_session(&outcome);
    sweep::ui::clean::print_outcome(&outcome, false);
    sweep::ui::clean::print_benchmark(&BenchmarkSample {
        before_free_bytes: before,
        after_free_bytes: after,
        elapsed_secs: elapsed,
        category_bytes: vec![],
    });
    try_recreate_reserve();
    Ok(())
}

#[cfg(not(windows))]
fn kill_processes(apps: &[sweep::domain::models::LockedProcess]) {
    let mut pids: Vec<u32> = Vec::new();
    let own = std::process::id();
    for a in apps {
        if a.pid == own || pids.contains(&a.pid) {
            continue;
        }
        pids.push(a.pid);
        println!("  killing {} (PID {})", a.name, a.pid);
        let _ = std::process::Command::new("kill")
            .args(["-9", &a.pid.to_string()])
            .output();
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
}

#[cfg(windows)]
fn run_ram(trim_top: Option<usize>, purge_standby: bool) -> Result<()> {
    use sweep::infra::win::ram::WinRamTrimmer;
    use sweep::services::ram_service::RamService;

    if trim_top.is_none() && !purge_standby {
        anyhow::bail!("nothing to do; pass --trim-top N and/or --purge-standby");
    }

    let mut svc = RamService::new(
        sweep::infra::sysinfo_monitor::SysinfoMonitor::new(),
        WinRamTrimmer::new(),
    );
    let report = svc.optimize(trim_top, purge_standby)?;
    sweep::ui::ram::print_ram_report(
        report.before.used_bytes,
        report.after.used_bytes,
        report.before.total_bytes,
        &report.outcome,
    );
    Ok(())
}

#[cfg(not(windows))]
fn run_ram(trim_top: Option<usize>, purge_standby: bool) -> Result<()> {
    use sweep::infra::linux::ram::LinuxRamTrimmer;
    use sweep::services::ram_service::RamService;

    if trim_top.is_some() {
        anyhow::bail!("per-process trim is not supported on linux; use --purge-standby");
    }
    if !purge_standby {
        anyhow::bail!("nothing to do; pass --purge-standby (needs root)");
    }
    let mut svc = RamService::new(
        sweep::infra::sysinfo_monitor::SysinfoMonitor::new(),
        LinuxRamTrimmer::new(),
    );
    let report = svc.optimize(None, true)?;
    sweep::ui::ram::print_ram_report(
        report.before.used_bytes,
        report.after.used_bytes,
        report.before.total_bytes,
        &report.outcome,
    );
    Ok(())
}

fn run_tui(top: usize) -> Result<()> {
    use sweep::services::system_service::SystemService;
    use sweep::services::usage_service::UsageService;
    use sweep::ui::tui::{Dashboard, DashboardData, TuiAction};

    let mut terminal = ratatui::init();

    let act = move |a: TuiAction| -> Result<String> {
        match a {
            TuiAction::TrimTop(n) => {
                #[cfg(windows)]
                {
                    use sweep::infra::win::ram::WinRamTrimmer;
                    use sweep::services::ram_service::RamService;
                    let mut svc =
                        RamService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new(), WinRamTrimmer::new());
                    let rep = svc.optimize(Some(n as usize), false)?;
                    let delta = rep.before.used_bytes.saturating_sub(rep.after.used_bytes);
                    Ok(format!(
                        "trimmed {n} processes, freed ~{}",
                        sweep::ui::status::fmt(delta)
                    ))
                }
                #[cfg(not(windows))]
                anyhow::bail!("trim not supported here")
            }
            TuiAction::PurgeStandby => {
                #[cfg(windows)]
                {
                    use sweep::domain::traits::RamTrimmer;
                    let mut t = sweep::infra::win::ram::WinRamTrimmer::new();
                    t.purge_standby()?;
                    Ok("standby list purged".into())
                }
                #[cfg(not(windows))]
                anyhow::bail!("purge not supported here")
            }
            // The modal already took the user's consent and the blocklist was
            // checked before it opened; KillService re-checks both anyway.
            TuiAction::Terminate {
                pid,
                name,
                size_bytes,
                mode,
            } => {
                use sweep::domain::models::KillRequest;
                use sweep::services::kill_service::KillService;

                let req = KillRequest {
                    pid,
                    name: name.clone(),
                    size_bytes,
                    mode,
                    consent: true,
                };
                if KillService::new().execute(&req) {
                    Ok(format!("terminated {name} (PID {pid})"))
                } else {
                    Ok(format!("{name} (PID {pid}) refused or already gone"))
                }
            }
        }
    };

    let dashboard = Dashboard::new(
        move || {
            let mut service = SystemService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new());
            let snap = service.status_report(top)?;
            let usage = UsageService::new(usage_probes()).collect_map();
            Ok(DashboardData { snap, usage })
        },
        || {
            use sweep::services::idle_service::{IdleConfig, IdleService};
            IdleService::new().detect_fast(&IdleConfig {
                top: 20,
                idle_mins: 5,
                min_write_mb: 10,
                clean_cache: false,
            })
        },
        act,
    );
    let result = dashboard.run(&mut terminal);
    ratatui::restore();
    result
}

fn run_bin(empty: bool, yes: bool) -> Result<()> {
    use sweep::domain::traits::RecycleBin;

    let bin = sweep::infra::trash_remover::TrashBin::new();
    let items = bin.items()?;
    println!("recycle bin: {} items", items.len());
    for i in items.iter().take(15) {
        println!("  {} ({})", i.name, i.original_parent);
    }
    if items.len() > 15 {
        println!("  ... and {} more", items.len() - 15);
    }

    if !empty {
        return Ok(());
    }
    if items.is_empty() {
        return Ok(());
    }
    if !(yes || sweep::ui::apps::confirm("permanently delete ALL items above?")) {
        println!("aborted");
        return Ok(());
    }
    ensure_headroom_or_consume_reserve();
    let before = free_bytes_on_index_volume();
    let start = Instant::now();
    let n = bin.purge_all()?;
    let elapsed = start.elapsed().as_secs_f64();
    let after = free_bytes_on_index_volume();
    println!("deleted {n} items");
    sweep::ui::clean::print_benchmark(&BenchmarkSample {
        before_free_bytes: before,
        after_free_bytes: after,
        elapsed_secs: elapsed,
        category_bytes: vec![],
    });
    try_recreate_reserve();
    Ok(())
}

fn run_dupes(min_mb: u64, trash_group: Option<usize>, yes: bool) -> Result<()> {
    use sweep::services::dup_service::{DupFinder, StdFileHasher};

    if !index_db_path().exists() {
        anyhow::bail!("no index yet; run `sweep index` first");
    }
    let store = open_store()?;
    let finder = DupFinder::new(store, StdFileHasher::new());
    let groups = finder.find(min_mb * 1024 * 1024, 200)?;
    if groups.is_empty() {
        println!("no duplicate groups found (>= {} MiB)", min_mb);
        return Ok(());
    }

    let mut wasted_total = 0u64;
    println!("  {:<4} {:>10} {:>12}  {}", "#", "SIZE", "WASTED", "PATHS (keeper first)");
    for (idx, g) in groups.iter().enumerate() {
        wasted_total += g.wasted_bytes;
        println!(
            "  {:<4} {:>10} {:>12}  {}",
            idx,
            fmt(g.size_bytes),
            fmt(g.wasted_bytes),
            truncate_path(&g.paths[0])
        );
        for p in &g.paths[1..] {
            println!("  {:<4} {:>10} {:>12}  {}", "", "", "", truncate_path(p));
        }
    }
    println!("\n{} groups, {} reclaimable", groups.len(), fmt(wasted_total));

    let Some(target) = trash_group else {
        return Ok(());
    };
    let Some(g) = groups.get(target) else {
        anyhow::bail!("group #{target} does not exist (0..={})", groups.len() - 1);
    };
    if !(yes || sweep::ui::apps::confirm(&format!(
        "move all but '{}' to the recycle bin? (frees ~{})",
        g.paths[0],
        fmt(g.wasted_bytes)
    ))) {
        println!("aborted");
        return Ok(());
    }
    let remover = sweep::infra::trash_remover::TrashRemover::new();
    use sweep::domain::traits::PathRemover as _;
    let mut removed = 0u64;
    for p in &g.paths[1..] {
        match remover.remove_path(std::path::Path::new(p)) {
            Ok(()) => removed += 1,
            Err(_) => println!("  failed (locked or missing): {p}"),
        }
    }
    println!("moved {removed} files to the recycle bin");
    Ok(())
}

fn truncate_path(p: &str) -> String {
    const MAX: usize = 90;
    if p.chars().count() <= MAX {
        p.to_string()
    } else {
        format!("…{}", &p[p.chars().count() - MAX..])
    }
}

fn run_schedule(
    install: bool,
    remove: bool,
    status: bool,
    guard_install: bool,
    guard_remove: bool,
    guard_status: bool,
) -> Result<()> {
    if guard_status {
        let installed = sweep::infra::schedule::guard_is_installed()?;
        println!("guard autostart: {}", if installed { "installed" } else { "not installed" });
        return Ok(());
    }
    if guard_install {
        return sweep::infra::schedule::guard_install();
    }
    if guard_remove {
        return sweep::infra::schedule::guard_remove();
    }
    if status {
        let installed = sweep::infra::schedule::is_installed()?;
        println!("scheduled indexing: {}", if installed { "installed" } else { "not installed" });
        return Ok(());
    }
    if install {
        return sweep::infra::schedule::install();
    }
    if remove {
        return sweep::infra::schedule::remove();
    }
    anyhow::bail!("nothing to do; pass --install, --remove or --status")
}

fn run_diagnose(
    deep: bool,
    config: Option<&std::path::Path>,
    rules: Option<&std::path::Path>,
) -> Result<()> {
    let store = open_store()?;
    sweep::ui::diagnose::run_diagnose(&store, deep, config, rules)?;
    Ok(())
}

fn run_status(top: usize) -> Result<()> {
    let _ = ensure_reserve();
    let mut service = SystemService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new());
    let snap = service.status_report(top)?;

    let usage_service = sweep::services::usage_service::UsageService::new(usage_probes());
    let usage_map = usage_service.collect_map();

    sweep::ui::status::print_status(&snap, Some(&usage_map))?;

    if let Some(store) = open_store_with_reserve() {
        if let Ok(stats) = store.stats() {
            println!(
                "\nindex: {} files, {} folders, {} cataloged (last run: {})",
                stats.files,
                stats.dirs,
                fmt(stats.total_bytes),
                store
                    .meta_get("last_run")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "never".to_string())
            );
        }
    } else if index_db_path().exists() {
        println!("\nindex: unavailable (disk full, reserve consumed — run sweep bin --empty)");
    } else {
        println!("\nindex: not built yet (run `sweep index`)");
    }
    Ok(())
}

fn run_index(status: bool, full: bool, roots: Vec<std::path::PathBuf>) -> Result<()> {
    let _ = ensure_reserve();
    let mut store = open_store()?;

    if status {
        let stats = store.stats()?;
        println!("index db: {}", index_db_path().display());
        println!("files: {}", stats.files);
        println!("folders: {}", stats.dirs);
        println!("cataloged size: {}", fmt(stats.total_bytes));
        println!(
            "last run: {}",
            store.meta_get("last_run")?.unwrap_or_else(|| "never".into())
        );
        return Ok(());
    }

    if full {
        store.clear()?;
        println!("cleared existing index");
    }

    let cfg = IndexConfig {
        roots: if roots.is_empty() {
            sweep::infra::walker::default_roots()
        } else {
            roots
        },
        walker: Default::default(),
    };

    println!("indexing roots:");
    for r in &cfg.roots {
        println!("  {}", r.display());
    }

    let mut svc = IndexService::new(store, cfg);
    let cancel = AtomicBool::new(false);
    let progress = Mutex::new(Default::default());
    let mut last_bucket = 0u64;
    let result = svc.run(&cancel, &progress, Some(&mut |p| {
        let bucket = p.dirs_scanned / 500;
        if bucket != last_bucket {
            last_bucket = bucket;
            print!(
                "\r  scanned {}, skipped {}, files {}   ",
                p.dirs_scanned, p.dirs_skipped, p.files_recorded
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }))?;

    println!();
    println!(
        "done: scanned {}, skipped {}, errors {}",
        result.dirs_scanned, result.dirs_skipped, result.errors
    );
    let stats = svc.stats()?;
    println!(
        "index now holds {} files / {} folders / {}",
        stats.files,
        stats.dirs,
        fmt(stats.total_bytes)
    );

    Ok(())
}

fn run_guard(
    ram_threshold: f64,
    disk_min_gb: u64,
    interval_secs: u64,
    once: bool,
    allow_service_stop: bool,
    allow_kill: bool,
) -> Result<()> {
    use sweep::domain::models::GuardConfig;
    use sweep::infra::sysinfo_monitor::SysinfoMonitor;
    use sweep::services::guard_service::{GuardLock, GuardLog, GuardService};

    let config = GuardConfig {
        ram_threshold,
        disk_min_gb,
        interval_secs,
        once,
        allow_service_stop,
        allow_kill,
    };

    let _lock = match GuardLock::acquire() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    GuardLog::info("guard started")?;
    let mut svc = GuardService::new(SysinfoMonitor::new(), config);
    svc.run()?;
    GuardLog::info("guard stopped")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_idle(
    top: usize,
    idle_mins: u64,
    min_write_mb: u64,
    clean_cache: bool,
    close: bool,
    kill: bool,
    force: bool,
    only: &[u32],
) -> Result<()> {
    use sweep::services::idle_service::{IdleConfig, IdleService};

    if kill && !force {
        anyhow::bail!("`--kill` requires `--force` (and still confirms each process)");
    }

    let config = IdleConfig {
        top,
        idle_mins,
        min_write_mb,
        clean_cache,
    };

    let svc = IdleService::new();
    // Termination targets specific PIDs, so skip the 60s sampling window.
    let offenders = if close || kill {
        svc.detect_fast(&config)?
    } else {
        svc.detect(&config)?
    };
    sweep::ui::idle::print_idle_table(&offenders);

    if close || kill {
        let targets: Vec<sweep::domain::models::KillRequest> = offenders
            .iter()
            .filter(|o| only.is_empty() || only.contains(&o.pid))
            .map(|o| sweep::domain::models::KillRequest {
                pid: o.pid,
                name: o.name.clone(),
                size_bytes: o.memory_bytes,
                mode: if kill {
                    sweep::domain::models::KillMode::Kill
                } else {
                    sweep::domain::models::KillMode::Close
                },
                consent: false,
            })
            .collect();
        terminate_with_consent(targets);
        return Ok(());
    }

    if clean_cache && !offenders.is_empty() {
        let freed = IdleService::clean_cache(&offenders)?;
        if freed > 0 {
            println!("cleaned {} of cache from offender processes", fmt(freed));
        }
    }

    Ok(())
}

/// `sweep bg` — list background processes by memory, with the same
/// consent-gated, blocklist-guarded termination as `sweep idle`.
fn run_bg(top: usize, kill: bool, force: bool, only: &[u32]) -> Result<()> {
    use sweep::services::system_service::SystemService;

    if kill && !force {
        anyhow::bail!("`--kill` requires `--force` (and still confirms each process)");
    }

    let mut service = SystemService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new());
    let snap = service.status_report(top)?;
    sweep::ui::idle::print_background_table(&snap.top_processes);

    if !kill {
        return Ok(());
    }
    let targets: Vec<sweep::domain::models::KillRequest> = snap
        .top_processes
        .iter()
        .filter(|p| only.is_empty() || only.contains(&p.pid))
        .map(|p| sweep::domain::models::KillRequest {
            pid: p.pid,
            name: p.name.clone(),
            size_bytes: p.memory_bytes,
            mode: sweep::domain::models::KillMode::Kill,
            consent: false,
        })
        .collect();
    terminate_with_consent(targets);
    Ok(())
}

/// Run each termination request through the blocklist and, for forced kills, an
/// explicit per-process confirmation prompt (FR-010, FR-011).
///
/// Nothing is ever terminated without passing both gates; every decision is
/// printed so the action is never silent (Principle II).
fn terminate_with_consent(targets: Vec<sweep::domain::models::KillRequest>) {
    use sweep::domain::models::KillMode;
    use sweep::services::kill_service::KillService;

    if targets.is_empty() {
        println!("no matching processes");
        return;
    }
    let svc = KillService::new();
    for mut req in targets {
        if KillService::is_blocked(&req) {
            println!(
                "  skipped {} (PID {}): protected system process",
                req.name, req.pid
            );
            continue;
        }
        if req.mode == KillMode::Kill {
            req.consent = sweep::ui::idle::confirm_kill_process(&req);
            if !req.consent {
                println!("  skipped {} (PID {}): not confirmed", req.name, req.pid);
                continue;
            }
        }
        let verb = if req.mode == KillMode::Kill {
            "killed"
        } else {
            "closed"
        };
        if svc.execute(&req) {
            println!("  {} {} (PID {})", verb, req.name, req.pid);
        } else {
            println!(
                "  {} (PID {}) already gone or could not be {}",
                req.name, req.pid, verb
            );
        }
    }
}
