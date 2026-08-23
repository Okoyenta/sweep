use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::collections::HashSet;

use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_SZ,
};

use crate::domain::models::InstalledApp;
use crate::domain::traits::AppInventory;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn uninstall_path() -> Vec<u16> {
    wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
}

pub struct RegistryAppInventory;

impl RegistryAppInventory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RegistryAppInventory {
    fn default() -> Self {
        Self::new()
    }
}

fn open_subkey(parent: HKEY, path: &[u16], extra: u32) -> Option<HKEY> {
    let mut out: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ | extra, &mut out) };
    if rc == ERROR_SUCCESS && !out.is_null() {
        Some(out)
    } else {
        None
    }
}

fn enum_subkeys(key: HKEY) -> Vec<String> {
    let mut names = Vec::new();
    for idx in 0..u32::MAX {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        let rc = unsafe {
            RegEnumKeyExW(
                key,
                idx,
                buf.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc == ERROR_NO_MORE_ITEMS || rc != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    names
}

fn read_value_raw(key: HKEY, name: &str) -> Option<(u32, Vec<u8>)> {
    let wname = wide(name);
    let mut kind: u32 = 0;
    let mut size: u32 = 0;
    let rc = unsafe {
        RegQueryValueExW(key, wname.as_ptr(), std::ptr::null(), &mut kind, std::ptr::null_mut(), &mut size)
    };
    if rc != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    let rc = unsafe {
        RegQueryValueExW(key, wname.as_ptr(), std::ptr::null(), std::ptr::null_mut(), data.as_mut_ptr() as *mut u8, &mut size)
    };
    if rc == ERROR_SUCCESS {
        Some((kind, data))
    } else {
        None
    }
}

fn read_string(key: HKEY, name: &str) -> Option<String> {
    let (kind, data) = read_value_raw(key, name)?;
    if kind != REG_SZ && kind != 1 {
        return None;
    }
    let pairs: Vec<u16> = data
        .chunks_exact(2)
        .take_while(|c| c[0] != 0 || c[1] != 0)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&pairs);
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn read_dword(key: HKEY, name: &str) -> Option<u32> {
    let (kind, data) = read_value_raw(key, name)?;
    if kind != REG_DWORD || data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn parse_app(key_name: &str, key: HKEY) -> Option<InstalledApp> {
    let name = read_string(key, "DisplayName")?;
    let _ = key_name;
    if read_dword(key, "SystemComponent").unwrap_or(0) == 1 {
        return None;
    }
    Some(InstalledApp {
        name,
        version: read_string(key, "DisplayVersion").unwrap_or_default(),
        publisher: read_string(key, "Publisher").unwrap_or_default(),
        install_location: read_string(key, "InstallLocation"),
        uninstall_command: read_string(key, "QuietUninstallString")
            .or_else(|| read_string(key, "UninstallString")),
        size_bytes: read_dword(key, "EstimatedSize").map(|kb| kb as u64 * 1024),
        last_run_unix: None,
    })
}

impl AppInventory for RegistryAppInventory {
    fn installed_apps(&self) -> anyhow::Result<Vec<InstalledApp>> {
        const HKLM_FLAGS: [(&str, u32); 2] =
            [("64-bit", KEY_WOW64_64KEY), ("32-bit", KEY_WOW64_32KEY)];

        let mut apps = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let roots: Vec<(HKEY, u32)> = {
            let mut v = vec![(HKEY_CURRENT_USER, 0u32)];
            for (_, flag) in HKLM_FLAGS {
                v.push((HKEY_LOCAL_MACHINE, flag));
            }
            v
        };

        for (root, extra) in roots {
            let Some(uninst) = open_subkey(root, &uninstall_path(), extra) else {
                continue;
            };
            for sub in enum_subkeys(uninst) {
                let sub_wide = wide(&sub);
                let Some(subkey) = open_subkey(uninst, &sub_wide, 0) else {
                    continue;
                };
                if let Some(app) = parse_app(&sub, subkey) {
                    let dedup_key = format!(
                        "{}|{}|{}",
                        app.name.to_lowercase(),
                        app.version,
                        app.size_bytes.unwrap_or(0)
                    );
                    if seen.insert(dedup_key) {
                        apps.push(app);
                    }
                }
                unsafe {
                    RegCloseKey(subkey);
                }
            }
            unsafe {
                RegCloseKey(uninst);
            }
        }

        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(apps)
    }
}
