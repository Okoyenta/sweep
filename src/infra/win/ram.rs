use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LUID, ERROR_NOT_ALL_ASSIGNED};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA,
};

use crate::domain::traits::RamTrimmer;

type RawHandle = *mut core::ffi::c_void;

unsafe extern "system" {
    fn NtSetSystemInformation(info_class: u32, info: *mut core::ffi::c_void, len: u32) -> i32;
}

const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 0x50;
const MEMORY_PURGE_STANDBY_LIST: u32 = 3;
const STATUS_SUCCESS: i32 = 0;
const STATUS_PRIVILEGE_NOT_HELD: i32 = -1073741727i32;

pub struct WinRamTrimmer;

impl WinRamTrimmer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinRamTrimmer {
    fn default() -> Self {
        Self::new()
    }
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

unsafe fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: RawHandle = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let mut luid: LUID = std::mem::zeroed();
        let name_w = wide(name);
        if LookupPrivilegeValueW(std::ptr::null(), name_w.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
        let ok = GetLastError() != ERROR_NOT_ALL_ASSIGNED;
        CloseHandle(token);
        ok
    }
}

impl RamTrimmer for WinRamTrimmer {
    fn trim_processes(&mut self, pids: &[u32]) -> anyhow::Result<(u32, u32)> {
        let mut succeeded = 0u32;
        let mut failed = 0u32;
        for &pid in pids {
            if pid == 0 {
                continue;
            }
            unsafe {
                let handle: RawHandle =
                    OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if handle.is_null() {
                    failed += 1;
                    continue;
                }
                if EmptyWorkingSet(handle) != 0 {
                    succeeded += 1;
                } else {
                    failed += 1;
                }
                CloseHandle(handle);
            }
        }
        Ok((succeeded, failed))
    }

    fn purge_standby(&mut self) -> anyhow::Result<bool> {
        unsafe {
            if !enable_privilege("SeProfileSingleProcessPrivilege") {
                anyhow::bail!("SeProfileSingleProcessPrivilege not held (run as admin)");
            }
            let mut command: u32 = MEMORY_PURGE_STANDBY_LIST;
            let status = NtSetSystemInformation(
                SYSTEM_MEMORY_LIST_INFORMATION,
                &mut command as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            match status {
                STATUS_SUCCESS => Ok(true),
                STATUS_PRIVILEGE_NOT_HELD => {
                    anyhow::bail!("standby purge denied (privilege not held)")
                }
                other => anyhow::bail!("standby purge failed with NTSTATUS {other:#x}"),
            }
        }
    }
}
