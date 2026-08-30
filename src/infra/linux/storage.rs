//! Linux storage probes and volume maintenance.
//!
//! Detects media type from `/sys/block/<dev>/queue/rotational` (the kernel's own
//! answer, no extra crate) and trims via `fstrim`. Rotational drives have no
//! general-purpose online defragmenter on Linux — ext4's `e4defrag` is
//! filesystem-specific and inappropriate to run unattended — so HDD maintenance
//! is reported as unsupported rather than approximated.
//!
//! Only the I/O lives here; the parsing of `/proc/mounts` and device names is in
//! `infra::storage_util`, which compiles and is tested on every platform.

use std::path::Path;

use crate::domain::models::{MediaType, VolumeInfo};
use crate::infra::storage_util::{device_for_mount, parent_block_device, parse_rotational};

/// Detect the media type backing a mount point.
///
/// Resolves mount -> block device via `/proc/mounts`, strips the partition
/// suffix to reach the parent device, then reads its `rotational` flag. Any
/// failure yields [`MediaType::Unknown`] so the caller refuses to act rather
/// than guessing.
pub fn media_type(mount: &str) -> MediaType {
    media_for_mount(&read_proc_mounts(), mount)
}

/// Resolve a mount's media type against already-read `/proc/mounts` content.
fn media_for_mount(mounts: &str, mount: &str) -> MediaType {
    let Some(base) = device_for_mount(mounts, mount)
        .as_deref()
        .and_then(parent_block_device)
    else {
        return MediaType::Unknown;
    };
    match std::fs::read_to_string(format!("/sys/block/{base}/queue/rotational")) {
        Ok(s) => parse_rotational(&s),
        Err(_) => MediaType::Unknown,
    }
}

fn read_proc_mounts() -> String {
    std::fs::read_to_string("/proc/mounts").unwrap_or_default()
}

/// List mounted volumes with their detected media type.
pub fn volumes() -> Vec<VolumeInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    // Read /proc/mounts once rather than per volume.
    let mounts = read_proc_mounts();
    disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|disk| {
            let mount = disk.mount_point().to_string_lossy().into_owned();
            let media = media_for_mount(&mounts, &mount);
            VolumeInfo { mount, media }
        })
        .collect()
}

/// Run `fstrim` against a mount point.
///
/// `dry_run` maps to `--dry-run`, which reports what would be discarded without
/// issuing the discards. Requires root; the failure text is surfaced to the
/// caller, which summarizes it into one actionable line.
pub fn fstrim(mount: &Path, dry_run: bool) -> anyhow::Result<String> {
    let mut cmd = std::process::Command::new("fstrim");
    cmd.arg("--verbose");
    if dry_run {
        cmd.arg("--dry-run");
    }
    cmd.arg(mount);
    let out = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("could not run fstrim: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        anyhow::bail!("fstrim failed: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolvable_mount_is_unknown_media() {
        assert_eq!(media_for_mount("", "/nonexistent"), MediaType::Unknown);
    }

    #[test]
    fn pseudo_filesystem_is_unknown_media() {
        let mounts = "proc /proc proc rw,nosuid 0 0\n";
        assert_eq!(media_for_mount(mounts, "/proc"), MediaType::Unknown);
    }
}
