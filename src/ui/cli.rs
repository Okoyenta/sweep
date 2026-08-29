use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sweep",
    version,
    about = "Monitor and free disk space and RAM"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
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
    },
}
