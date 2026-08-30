//! Windows storage probes and volume maintenance.
//!
//! Detects the physical media behind a drive letter by asking the device
//! whether it incurs a seek penalty, then runs `Optimize-Volume` with the flag
//! that matches. Both operations degrade gracefully: an undetectable media type
//! is reported as `Unknown` and no maintenance is attempted, rather than
//! guessing.
//!
//! Media detection uses a direct `IOCTL_STORAGE_QUERY_PROPERTY` call rather than
//! the `Get-PhysicalDisk` PowerShell cmdlet: loading the Storage module costs
//! tens of seconds on a cold shell, while the ioctl answers instantly and needs
//! no elevation (Constitution Principle I — declaration-only FFI, no idle cost).

use std::process::Command;

use crate::domain::models::{MediaType, VolumeInfo};

/// `IOCTL_STORAGE_QUERY_PROPERTY` control code.
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
/// `StorageDeviceSeekPenaltyProperty` in the `STORAGE_PROPERTY_ID` enum.
const STORAGE_DEVICE_SEEK_PENALTY_PROPERTY: u32 = 7;
/// `PropertyStandardQuery` in the `STORAGE_QUERY_TYPE` enum.
const PROPERTY_STANDARD_QUERY: u32 = 0;

/// `STORAGE_PROPERTY_QUERY` input buffer.
#[repr(C)]
struct StoragePropertyQuery {
    property_id: u32,
    query_type: u32,
    additional_parameters: [u8; 1],
}

/// `DEVICE_SEEK_PENALTY_DESCRIPTOR` output buffer.
#[repr(C)]
struct DeviceSeekPenaltyDescriptor {
    version: u32,
    size: u32,
    incurs_seek_penalty: u8,
}

/// Detect the media type behind a drive letter such as `C`.
///
/// Opens the volume with zero access rights (a query handle needs no
/// permissions) and asks whether the underlying device incurs a seek penalty:
/// rotational media does, solid-state does not. Any failure — a network or
/// virtual volume, a device that does not answer the query — yields
/// [`MediaType::Unknown`] so the caller refuses to act rather than guessing.
pub fn media_type(drive_letter: char) -> MediaType {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    // `\\.\C:` addresses the volume itself; note the deliberate lack of a
    // trailing separator, which would name the filesystem root instead.
    let path = format!(r"\\.\{drive_letter}:");
    let wide: Vec<u16> = std::ffi::OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            0, // query only: no read/write access required
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return MediaType::Unknown;
        }

        let query = StoragePropertyQuery {
            property_id: STORAGE_DEVICE_SEEK_PENALTY_PROPERTY,
            query_type: PROPERTY_STANDARD_QUERY,
            additional_parameters: [0],
        };
        let mut descriptor = DeviceSeekPenaltyDescriptor {
            version: 0,
            size: 0,
            incurs_seek_penalty: 0,
        };
        let mut returned: u32 = 0;

        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<StoragePropertyQuery>() as u32,
            &mut descriptor as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<DeviceSeekPenaltyDescriptor>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        );
        CloseHandle(handle);

        if ok == 0 || returned as usize != std::mem::size_of::<DeviceSeekPenaltyDescriptor>() {
            return MediaType::Unknown;
        }
        seek_penalty_to_media(descriptor.incurs_seek_penalty)
    }
}

/// Map a `DEVICE_SEEK_PENALTY_DESCRIPTOR.IncursSeekPenalty` value to sweep's
/// media enum: a seek penalty means rotational platters.
fn seek_penalty_to_media(incurs_seek_penalty: u8) -> MediaType {
    if incurs_seek_penalty == 0 {
        MediaType::Ssd
    } else {
        MediaType::Hdd
    }
}

/// List fixed volumes with their detected media type.
pub fn volumes() -> Vec<VolumeInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for disk in disks.list() {
        if disk.total_space() == 0 {
            continue;
        }
        let mount = disk.mount_point().to_string_lossy().into_owned();
        let Some(letter) = drive_letter(&mount) else {
            continue;
        };
        out.push(VolumeInfo {
            mount,
            media: media_type(letter),
        });
    }
    out
}

/// Extract the drive letter from a mount point like `C:\`.
pub fn drive_letter(mount: &str) -> Option<char> {
    let c = mount.chars().next()?;
    if c.is_ascii_alphabetic() && mount.chars().nth(1) == Some(':') {
        Some(c.to_ascii_uppercase())
    } else {
        None
    }
}

/// Run `Optimize-Volume` on a drive letter.
///
/// `flag` is the operation switch (`ReTrim`, `Defrag`, or `Analyze`). Returns
/// the combined tool output on success, or an error describing the failure —
/// most commonly missing elevation, which `Optimize-Volume` requires.
pub fn optimize_volume(drive_letter: char, flag: &str) -> anyhow::Result<String> {
    let script = format!(
        "Optimize-Volume -DriveLetter {drive_letter} -{flag} -Verbose 2>&1 | Out-String"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("could not run Optimize-Volume: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        anyhow::bail!("Optimize-Volume failed: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_seek_penalty_is_solid_state() {
        assert_eq!(seek_penalty_to_media(0), MediaType::Ssd);
    }

    #[test]
    fn seek_penalty_is_rotational() {
        // BOOLEAN is any non-zero value, not strictly 1.
        assert_eq!(seek_penalty_to_media(1), MediaType::Hdd);
        assert_eq!(seek_penalty_to_media(0xFF), MediaType::Hdd);
    }

    // Live probe: queries the real system volume. Ignored on CI, where the
    // filesystem may be virtualized and answer Unknown.
    #[test]
    #[ignore]
    fn live_system_volume_reports_a_media_type() {
        assert_ne!(media_type('C'), MediaType::Unknown);
    }

    #[test]
    fn extracts_drive_letter_from_mount() {
        assert_eq!(drive_letter("C:\\"), Some('C'));
        assert_eq!(drive_letter("d:\\"), Some('D'));
        assert_eq!(drive_letter("\\\\server\\share"), None);
        assert_eq!(drive_letter(""), None);
    }
}
