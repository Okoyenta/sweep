//! Platform-neutral parsing helpers for storage detection.
//!
//! The Linux media probe reads `/proc/mounts` and `/sys/block/*/queue/rotational`,
//! but the *parsing* of those files is pure string handling. It lives here rather
//! than in `infra/linux/` so it compiles and is unit-tested on every platform:
//! cross-compiling to Linux is not possible on a Windows dev box (bundled SQLite
//! needs `x86_64-linux-gnu-gcc`), so logic hidden behind `#[cfg(not(windows))]`
//! would otherwise reach CI completely unchecked.

use crate::domain::models::MediaType;

/// Find the device backing `mount` in `/proc/mounts` content.
///
/// Picks the longest matching mount point so a path under `/home` resolves to
/// the `/home` device rather than to `/`. Pseudo-filesystems (`proc`, `tmpfs`)
/// are ignored because only real `/dev/*` nodes have a rotational flag.
pub fn device_for_mount(mounts: &str, mount: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut cols = line.split_whitespace();
        let (Some(dev), Some(point)) = (cols.next(), cols.next()) else {
            continue;
        };
        // An exact match settles it either way. A pseudo-filesystem mounted
        // right here (tmpfs on /run, proc on /proc) is backed by no block
        // device at all, so it must resolve to nothing rather than falling
        // through to whichever real device happens to contain the path.
        if mount == point {
            return dev.starts_with("/dev/").then(|| dev.to_string());
        }
        if !dev.starts_with("/dev/") {
            continue;
        }
        if mount.starts_with(point) && best.as_ref().is_none_or(|(len, _)| point.len() > *len) {
            best = Some((point.len(), dev.to_string()));
        }
    }
    best.map(|(_, d)| d)
}

/// Reduce a partition device path to its parent block device name.
///
/// `/dev/sda2` -> `sda`, `/dev/nvme0n1p3` -> `nvme0n1`, `/dev/mmcblk0p1` ->
/// `mmcblk0`. Devices using the `p<N>` partition convention are handled
/// separately from those with plain trailing digits, because `nvme0n1` ends in a
/// digit that is part of the device name rather than a partition number.
pub fn parent_block_device(dev: &str) -> Option<String> {
    let name = dev.strip_prefix("/dev/")?;
    if name.is_empty() {
        return None;
    }
    if name.starts_with("nvme") || name.starts_with("mmcblk") || name.starts_with("loop") {
        if let Some(idx) = name.rfind('p') {
            if idx > 0 && name[idx + 1..].chars().all(|c| c.is_ascii_digit())
                && !name[idx + 1..].is_empty()
            {
                return Some(name[..idx].to_string());
            }
        }
        return Some(name.to_string());
    }
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if base.is_empty() {
        None
    } else {
        Some(base.to_string())
    }
}

/// Interpret the kernel `rotational` flag (1 = spinning platters, 0 = solid state).
pub fn parse_rotational(raw: &str) -> MediaType {
    match raw.trim() {
        "0" => MediaType::Ssd,
        "1" => MediaType::Hdd,
        _ => MediaType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/sda1 /home ext4 rw,relatime 0 0
proc /proc proc rw,nosuid 0 0
tmpfs /run tmpfs rw,nosuid 0 0
";

    #[test]
    fn parses_rotational_flag() {
        assert_eq!(parse_rotational("0\n"), MediaType::Ssd);
        assert_eq!(parse_rotational("1\n"), MediaType::Hdd);
        assert_eq!(parse_rotational(""), MediaType::Unknown);
        assert_eq!(parse_rotational("garbage"), MediaType::Unknown);
    }

    #[test]
    fn resolves_device_for_exact_mount() {
        assert_eq!(
            device_for_mount(MOUNTS, "/home").as_deref(),
            Some("/dev/sda1")
        );
        assert_eq!(
            device_for_mount(MOUNTS, "/").as_deref(),
            Some("/dev/nvme0n1p2")
        );
    }

    #[test]
    fn prefers_longest_matching_mount() {
        // A path under /home must resolve to the /home device, not to /.
        assert_eq!(
            device_for_mount(MOUNTS, "/home/me/cache").as_deref(),
            Some("/dev/sda1")
        );
    }

    #[test]
    fn ignores_pseudo_filesystems() {
        assert_eq!(device_for_mount(MOUNTS, "/proc"), None);
        assert_eq!(device_for_mount(MOUNTS, "/run"), None);
    }

    #[test]
    fn unknown_mount_resolves_to_nothing() {
        assert_eq!(device_for_mount("", "/"), None);
    }

    #[test]
    fn strips_partition_suffix() {
        assert_eq!(parent_block_device("/dev/sda2").as_deref(), Some("sda"));
        assert_eq!(parent_block_device("/dev/vdb1").as_deref(), Some("vdb"));
        assert_eq!(
            parent_block_device("/dev/nvme0n1p3").as_deref(),
            Some("nvme0n1")
        );
        assert_eq!(
            parent_block_device("/dev/mmcblk0p1").as_deref(),
            Some("mmcblk0")
        );
    }

    #[test]
    fn whole_disk_device_is_unchanged() {
        // nvme0n1 ends in a digit that belongs to the device, not a partition.
        assert_eq!(parent_block_device("/dev/sda").as_deref(), Some("sda"));
        assert_eq!(
            parent_block_device("/dev/nvme0n1").as_deref(),
            Some("nvme0n1")
        );
        assert_eq!(
            parent_block_device("/dev/mmcblk0").as_deref(),
            Some("mmcblk0")
        );
    }

    #[test]
    fn malformed_device_paths_are_rejected() {
        assert_eq!(parent_block_device("sda2"), None);
        assert_eq!(parent_block_device("/dev/"), None);
        assert_eq!(parent_block_device("/dev/123"), None);
    }
}
