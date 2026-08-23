pub mod schedule;
pub mod sysinfo_monitor;
pub mod paths;
pub mod sqlite_store;
pub mod trash_remover;
pub mod walker;

#[cfg(windows)]
pub mod win;

#[cfg(not(windows))]
pub mod linux;
