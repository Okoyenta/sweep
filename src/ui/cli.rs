use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
// `version` is handled by sweep rather than clap so `--version` can append an
// update hint after querying GitHub Releases (FR-019).
#[command(
    name = "sweep",
    disable_version_flag = true,
    about = "Monitor and free disk space and RAM"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Print the version and, when online, whether a newer release exists.
    #[arg(short = 'V', long)]
    pub version: bool,
    /// Path to a `sweep.toml` overriding the CWD / user-config lookup.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Additional cleaner rule-pack TOML to load alongside `sweep.toml`.
    #[arg(long, global = true, value_name = "PATH")]
    pub rules: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    Status {
        #[arg(short, long, default_value_t = 10)]
        top: usize,
    },
    Index {
        #[arg(long)]
        status: bool,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        roots: Vec<PathBuf>,
    },
    Apps {
        #[arg(long)]
        since_days: Option<u64>,
        #[arg(long)]
        uninstall: Option<String>,
    },
    Clean {
        #[arg(long)]
        scan_only: bool,
        #[arg(long, num_args = 0..)]
        only: Vec<String>,
        #[arg(short, long)]
        yes: bool,
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        stop_services: bool,
        #[arg(long)]
        kill: bool,
    },
    Ram {
        #[arg(short, long)]
        trim_top: Option<usize>,
        #[arg(short, long)]
        purge_standby: bool,
    },
    Tui {
        #[arg(short, long, default_value_t = 10)]
        top: usize,
    },
    Bin {
        #[arg(long)]
        empty: bool,
        #[arg(short, long)]
        yes: bool,
    },
    Dupes {
        #[arg(long, default_value_t = 1)]
        min_mb: u64,
        #[arg(long)]
        trash_group: Option<usize>,
        #[arg(short, long)]
        yes: bool,
    },
    Diagnose {
        #[arg(long)]
        deep: bool,
    },
    Schedule {
        #[arg(long)]
        install: bool,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        status: bool,
        #[arg(long)]
        guard_install: bool,
        #[arg(long)]
        guard_remove: bool,
        #[arg(long)]
        guard_status: bool,
    },
    Guard {
        #[arg(long, default_value_t = 0.90)]
        ram_threshold: f64,
        #[arg(long, default_value_t = 10)]
        disk_min_gb: u64,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long)]
        once: bool,
        #[arg(long)]
        allow_service_stop: bool,
        #[arg(long)]
        allow_kill: bool,
    },
    Idle {
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long, default_value_t = 5)]
        idle_mins: u64,
        #[arg(long, default_value_t = 10)]
        min_write_mb: u64,
        #[arg(long)]
        clean_cache: bool,
        /// Gracefully close the listed offenders (WM_CLOSE / SIGTERM).
        #[arg(long)]
        close: bool,
        /// Forcibly kill the listed offenders; requires `--force` and confirmation.
        #[arg(long)]
        kill: bool,
        /// Required alongside `--kill`; each process is still confirmed individually.
        #[arg(long)]
        force: bool,
        /// Restrict `--close` / `--kill` to these PIDs.
        #[arg(long, num_args = 0..)]
        only: Vec<u32>,
    },
    /// Pre-flight safety report: reserve, elevation, toast, guard, would-clean.
    Doctor,
    /// Maintain drives: TRIM solid-state volumes, defragment rotational ones.
    Optimize {
        /// Volume to maintain (e.g. `C:\` or `/home`); omit to list all volumes.
        #[arg(long, value_name = "MOUNT")]
        volume: Option<String>,
        /// Analyze only — report what would happen without modifying anything.
        #[arg(long)]
        analyze: bool,
        /// Skip the confirmation prompt.
        #[arg(short, long)]
        yes: bool,
    },
    /// Restore the most recent session of trashed items from the Recycle Bin.
    Undo,
    /// Background process view, with the same consent-gated termination as `idle`.
    Bg {
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Forcibly kill the selected processes; requires `--force` and confirmation.
        #[arg(long)]
        kill: bool,
        /// Required alongside `--kill`; each process is still confirmed individually.
        #[arg(long)]
        force: bool,
        /// Restrict `--kill` to these PIDs.
        #[arg(long, num_args = 0..)]
        only: Vec<u32>,
    },
}
