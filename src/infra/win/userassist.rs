use windows_sys::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};

use crate::domain::models::{AppUsage, UsageSource};
use crate::domain::traits::UsageProbe;

const USERASSIST_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist";
const COUNT_SUBKEY: &str = "Count";
const BLOB_MIN_LEN: usize = 0x44;
const RUNTIME_OFFSET: usize = 0x3C;
const FT_TO_UNIX_SECS: i64 = 11_644_473_600;

pub fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' => ((c as u8 - b'A' + 13) % 26 + b'A') as char,
            'a'..='z' => ((c as u8 - b'a' + 13) % 26 + b'a') as char,
            other => other,
        })
        .collect()
}

pub fn filetime_to_unix(ft: i64) -> i64 {
    ft / 10_000_000 - FT_TO_UNIX_SECS
}

pub fn parse_count_blob(data: &[u8]) -> Option<(u32, i64)> {
    if data.len() < BLOB_MIN_LEN {
        return None;
    }
    let run_count = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let ft = i64::from_le_bytes(
        data[RUNTIME_OFFSET..RUNTIME_OFFSET + 8]
            .try_into()
            .ok()?,
    );
    if ft <= 0 {
        return None;
    }
    Some((run_count, filetime_to_unix(ft)))
}

pub fn exe_name_from_path(path: &str) -> Option<String> {
    let name = std::path::Path::new(path).file_name()?.to_str()?;
    if !name.to_lowercase().ends_with(".exe") {
        return None;
    }
    Some(name.to_lowercase())
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn open_key(parent: HKEY, path: &str) -> Option<HKEY> {
    let mut hk: HKEY = std::ptr::null_mut();
    let wide = to_wide(path);
    let err = unsafe { RegOpenKeyExW(parent, wide.as_ptr(), 0, KEY_READ, &mut hk) };
    if err == 0 {
        Some(hk)
    } else {
        None
    }
}

fn enum_subkeys(hk: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx: u32 = 0;
    loop {
        let mut name_buf = vec![0u16; 256];
        let mut name_len: u32 = name_buf.len() as u32;
        let err = unsafe {
            RegEnumKeyExW(
                hk,
                idx,
                name_buf.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        if err != 0 {
            idx += 1;
            if idx > 10_000 {
                break;
            }
            continue;
        }
        out.push(from_wide(&name_buf));
        idx += 1;
    }
    out
}

struct RawValue {
    name: String,
    data: Vec<u8>,
}

fn enum_values(hk: HKEY) -> Vec<RawValue> {
    let mut out = Vec::new();
    let mut idx: u32 = 0;
    loop {
        let mut name_buf = vec![0u16; 16384];
        let mut name_len: u32 = name_buf.len() as u32;
        let mut data = vec![0u8; 1024];
        let mut data_len: u32 = data.len() as u32;
        let err = unsafe {
            RegEnumValueW(
                hk,
                idx,
                name_buf.as_mut_ptr(),
                &mut name_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                data.as_mut_ptr(),
                &mut data_len,
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        if err != 0 || data_len as usize > data.len() {
            idx += 1;
            if idx > 100_000 {
                break;
            }
            continue;
        }
        data.truncate(data_len as usize);
        out.push(RawValue {
            name: from_wide(&name_buf),
            data,
        });
        idx += 1;
    }
    out
}

pub struct UserAssistProbe;

impl UserAssistProbe {
    pub fn new() -> Self {
        Self
    }

    fn collect(&self) -> anyhow::Result<Vec<AppUsage>> {
        let mut out = Vec::new();
        let Some(ua_key) = open_key(HKEY_CURRENT_USER, USERASSIST_PATH) else {
            return Ok(out);
        };

        for guid in enum_subkeys(ua_key) {
            let Some(count_key) = open_key(ua_key, &format!("{guid}\\{COUNT_SUBKEY}")) else {
                continue;
            };
            for value in enum_values(count_key) {
                if value.data.is_empty() {
                    continue;
                }
                let decoded = rot13(&value.name);
                let Some(exe) = exe_name_from_path(&decoded) else {
                    continue;
                };
                let Some((run_count, ts)) = parse_count_blob(&value.data) else {
                    continue;
                };
                out.push(AppUsage {
                    exe_name: exe,
                    last_run_unix: ts,
                    run_count: run_count as u64,
                    source: UsageSource::UserAssist,
                });
            }
            unsafe {
                RegCloseKey(count_key);
            }
        }
        unsafe {
            RegCloseKey(ua_key);
        }
        Ok(out)
    }
}

impl Default for UserAssistProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageProbe for UserAssistProbe {
    fn probe(&self) -> anyhow::Result<Vec<AppUsage>> {
        self.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rot13_roundtrip() {
        let original = r"C:\Users\test\AppData\Local\App.EXE";
        assert_eq!(rot13(&rot13(original)), original);
        assert_eq!(rot13("CHROME.EXE"), "PUEBZR.RKR");
    }

    #[test]
    fn parses_blob_with_known_filetime() {
        let ft: i64 = 133_500_000_000_000_000;
        let mut blob = vec![0u8; 72];
        blob[0..4].copy_from_slice(&42u32.to_le_bytes());
        blob[RUNTIME_OFFSET..RUNTIME_OFFSET + 8].copy_from_slice(&ft.to_le_bytes());
        let (count, ts) = parse_count_blob(&blob).unwrap();
        assert_eq!(count, 42);
        assert_eq!(ts, filetime_to_unix(ft));
        assert_eq!(ts, 1_705_526_400);
    }

    #[test]
    fn rejects_short_or_empty_blobs() {
        assert!(parse_count_blob(&[]).is_none());
        assert!(parse_count_blob(&vec![0u8; 16]).is_none());
        let mut zeroed = vec![0u8; 72];
        assert!(parse_count_blob(&zeroed).is_none());
        zeroed[RUNTIME_OFFSET] = 1;
        assert!(parse_count_blob(&zeroed).is_some());
    }

    #[test]
    fn extracts_exe_names_from_decoded_paths() {
        assert_eq!(
            exe_name_from_path(r"C:\Program Files\Git\git-bash.exe"),
            Some("git-bash.exe".to_string())
        );
        assert_eq!(exe_name_from_path(r"C:\docs\readme.txt"), None);
        assert_eq!(exe_name_from_path("not-a-path"), None);
    }

    #[test]
    fn filetime_epoch_is_sane() {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let now_ft = (now_unix + FT_TO_UNIX_SECS) * 10_000_000;
        assert!((filetime_to_unix(now_ft) - now_unix).abs() <= 1);
    }
}
