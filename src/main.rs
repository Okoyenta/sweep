use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use anyhow::{Context, Result};
use clap::Parser;
use sweep::domain::traits::{IndexStore, UsageProbe};
use sweep::infra::paths::index_db_path;
use sweep::infra::sqlite_store::SqliteStore;
use sweep::services::index_service::{IndexConfig, IndexService};
use sweep::services::system_service::SystemService;
use sweep::ui::cli::{Cli, Command};
use sweep::ui::status::fmt;

fn main() -> Result<()> {
    let cli = Cli::parse();
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
        }) => run_clean(scan_only, only, yes),
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
        Some(Command::Schedule {
            install,
            remove,
            status,
        }) => run_schedule(install, remove, status),
        None => run_status(10),
    }
}

fn open_store() -> Result<SqliteStore> {
    SqliteStore::open(&index_db_path()).context("opening index database")
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
fn run_clean(scan_only: bool, only: Vec<String>, yes: bool) -> Result<()> {
    use sweep::services::clean_service::CleanService;

    let categories = sweep::infra::win::clean_paths::discover_categories();
    let svc = CleanService::new(sweep::infra::trash_remover::TrashRemover::new());
    let scans = svc.scan(&categories);

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

    let outcome = svc.run(&scans, Some(&only))?;
    sweep::ui::clean::print_outcome(&outcome, false);
    Ok(())
}

#[cfg(not(windows))]
fn run_clean(scan_only: bool, only: Vec<String>, yes: bool) -> Result<()> {
    use sweep::infra::linux::clean_paths;
    use sweep::services::clean_service::CleanService;

    let categories = clean_paths::discover_categories();
    let svc = CleanService::new(sweep::infra::trash_remover::TrashRemover::new());
    let scans = svc.scan(&categories);

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
    let outcome = svc.run(&scans, Some(&only))?;
    sweep::ui::clean::print_outcome(&outcome, false);
    Ok(())
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
        }
    };

    let dashboard = Dashboard::new(
        move || {
            let mut service = SystemService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new());
            let snap = service.status_report(top)?;
            let usage = UsageService::new(usage_probes()).collect_map();
            Ok(DashboardData { snap, usage })
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
    let n = bin.purge_all()?;
    println!("deleted {n} items");
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

fn run_schedule(install: bool, remove: bool, status: bool) -> Result<()> {
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

fn run_status(top: usize) -> Result<()> {
    let mut service = SystemService::new(sweep::infra::sysinfo_monitor::SysinfoMonitor::new());
    let snap = service.status_report(top)?;

    let usage_service = sweep::services::usage_service::UsageService::new(usage_probes());
    let usage_map = usage_service.collect_map();

    sweep::ui::status::print_status(&snap, Some(&usage_map))?;

    if index_db_path().exists() {
        let store = open_store()?;
        let stats = store.stats()?;
        println!(
            "\nindex: {} files, {} folders, {} cataloged (last run: {})",
            stats.files,
            stats.dirs,
            fmt(stats.total_bytes),
            store
                .meta_get("last_run")?
                .unwrap_or_else(|| "never".to_string())
        );
    } else {
        println!("\nindex: not built yet (run `sweep index`)");
    }
    Ok(())
}

fn run_index(status: bool, full: bool, roots: Vec<std::path::PathBuf>) -> Result<()> {
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
