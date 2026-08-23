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
    Schedule {
        #[arg(long)]
        install: bool,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        status: bool,
    },
}
