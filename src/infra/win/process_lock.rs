use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::domain::models::LockedProcess;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileType, GetFinalPathNameByHandleW, FILE_TYPE_DISK,
};
use windows_sys::Win32::System::Threading::OpenProcess;

use sysinfo::{ProcessesToUpdate, System};

/// NT system-information class for the extended handle table snapshot.
const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 64;

static FILE_TYPE_INDEX: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();

#[repr(C)]
struct SystemHandleTableEntryInfoEx {
    object: usize,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

#[repr(C)]
struct SystemHandleInformationEx {
    number_of_handles: usize,
    reserved: usize,
    handles: [SystemHandleTableEntryInfoEx; 1],
}

unsafe impl Send for SystemHandleTableEntryInfoEx {}
unsafe impl Sync for SystemHandleTableEntryInfoEx {}
unsafe impl Send for SystemHandleInformationEx {}
unsafe impl Sync for SystemHandleInformationEx {}

unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

unsafe extern "system" {
    fn DuplicateHandle(
        h_source_process_handle: HANDLE,
        h_source_handle: HANDLE,
        h_target_process_handle: HANDLE,
        lp_target_handle: *mut HANDLE,
        dw_desired_access: u32,
        b_inherit_handle: i32,
        dw_options: u32,
    ) -> i32;
    fn GetCurrentProcess() -> HANDLE;
    fn GetCurrentProcessId() -> u32;
}

pub fn locked_processes(paths: &[&Path]) -> Vec<LockedProcess> {
    if paths.is_empty() {
        return Vec::new();
    }
    let targets: Vec<String> = paths
        .iter()
        .filter_map(|p| canonical(p).and_then(|c| c.to_str().map(str::to_owned)))
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    // Normalized lowercased targets for comparison + early-exit bookkeeping.
    let normalized_targets: HashSet<String> = targets.iter().map(|t| normalize_target(t)).collect();

    let idx = match file_type_index() {
        Some(v) => v,
        None => return Vec::new(),
    };

    let buffer = match query_handles() {
        Some(b) => b,
        None => return Vec::new(),
    };
    let ptr = buffer.as_ptr() as *const SystemHandleInformationEx;
    let count = unsafe { (*ptr).number_of_handles };
    let entries = unsafe { core::slice::from_raw_parts((*ptr).handles.as_ptr(), count) };

    let mut proc_cache: HashMap<u32, HANDLE> = HashMap::new();
    let current_pid = unsafe { GetCurrentProcessId() };
    let current_proc = unsafe { GetCurrentProcess() };
    let mut found: Vec<LockedProcess> = Vec::new();
    let mut found_normalized: HashSet<String> = HashSet::new();
    let mut names = ProcessNames::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut resolved: usize = 0;

    for entry in entries {
        if Instant::now() > deadline {
            break;
        }
        if entry.object_type_index != idx {
            continue;
        }
        let pid = entry.unique_process_id as u32;
        if pid == 0 {
            continue;
        }
        // Early exit: all distinct target paths have been seen at least once.
        if !normalized_targets.is_empty() && found_normalized.len() == normalized_targets.len() {
            // Still need to keep scanning? For single target, break. For multiple, we have all.
            break;
        }
        let handle_value = entry.handle_value;
        // Get or open source process handle (cached).
        let proc_handle = if pid == current_pid {
            current_proc
        } else {
            match proc_cache.get(&pid) {
                Some(&h) => h,
                None => {
                    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
                    const PROCESS_DUP_HANDLE: u32 = 0x0040;
                    let h = unsafe {
                        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_DUP_HANDLE, 0, pid)
                    };
                    proc_cache.insert(pid, h);
                    h
                }
            }
        };
        if proc_handle.is_null() {
            continue;
        }
        if let Some(path) = resolve_with_handle(proc_handle, handle_value) {
            resolved += 1;
            let norm = normalize_target(&path);
            if normalized_targets.contains(&norm) {
                // Map normalized back to original target string (first match).
                if let Some(orig) = targets.iter().find(|t| normalize_target(t) == norm) {
                    found_normalized.insert(norm.clone());
                    found.push(LockedProcess {
                        pid,
                        name: names.name_of(pid),
                        path: PathBuf::from(orig.clone()),
                    });
                }
            }
        }
    }

    // Close cached handles (skip current process pseudo-handle).
    for (_, h) in proc_cache {
        if !h.is_null() {
            unsafe { CloseHandle(h) };
        }
    }
    let _ = resolved;
    dedup(found)
}

fn query_handles() -> Option<Vec<u8>> {
    let mut size: usize = 1024 * 1024;
    loop {
        let mut buf = vec![0u8; size];
        let mut needed: u32 = 0;
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                size as u32,
                &mut needed,
            )
        };
        if status == 0 {
            return Some(buf);
        }
        if (status as u32) == 0xC000_0004 {
            size = size.saturating_mul(2).max(needed as usize).max(size + 1);
            if size > (512 * 1024 * 1024) {
                return None;
            }
            continue;
        }
        return None;
    }
}

fn file_type_index() -> Option<u16> {
    *FILE_TYPE_INDEX.get_or_init(|| {
        let me = unsafe { GetCurrentProcessId() };
        let marker = std::env::temp_dir().join(format!("sweep-type-{me}.tmp"));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&marker)
            .ok()?;
        std::io::Write::write_all(&mut f, b"x").ok()?;

        let found = (|| {
            let buffer = query_handles()?;
            let ptr = buffer.as_ptr() as *const SystemHandleInformationEx;
            let count = unsafe { (*ptr).number_of_handles };
            let entries = unsafe { core::slice::from_raw_parts((*ptr).handles.as_ptr(), count) };
            let current_proc = unsafe { GetCurrentProcess() };
            for e in entries {
                if e.unique_process_id as u32 != me {
                    continue;
                }
                if let Some(path) = resolve_with_handle(current_proc, e.handle_value) {
                    if paths_equal(&path, marker.to_str()?) {
                        return Some(e.object_type_index);
                    }
                }
            }
            None
        })();

        drop(f);
        let _ = std::fs::remove_file(&marker);
        found
    })
}

#[allow(dead_code)]
fn resolve_path(pid: u32, handle_value: usize) -> Option<String> {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_DUP_HANDLE: u32 = 0x0040;
    let proc_handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_DUP_HANDLE, 0, pid) };
    if proc_handle.is_null() {
        return None;
    }
    let res = resolve_with_handle(proc_handle, handle_value);
    unsafe { CloseHandle(proc_handle) };
    res
}

fn resolve_with_handle(proc_handle: HANDLE, handle_value: usize) -> Option<String> {
    let mut dup: HANDLE = core::ptr::null_mut();
    let ok = unsafe {
        DuplicateHandle(
            proc_handle,
            handle_value as HANDLE,
            GetCurrentProcess(),
            &mut dup,
            0,
            0,
            0,
        )
    };
    if ok == 0 || dup.is_null() {
        return None;
    }
    // Fast filter: only disk files can be our cache/temp targets. This avoids
    // calling GetFinalPathNameByHandleW on pipes, char devices, etc., which
    // can block for seconds on some handles.
    let ft = unsafe { GetFileType(dup) };
    if ft != FILE_TYPE_DISK {
        unsafe { CloseHandle(dup) };
        return None;
    }
    let mut cap = 512usize;
    loop {
        let mut buf = vec![0u16; cap];
        let len = unsafe { GetFinalPathNameByHandleW(dup, buf.as_mut_ptr() as *mut u16, cap as u32, 0) };
        if len == 0 {
            unsafe { CloseHandle(dup) };
            return None;
        }
        if (len as usize) < cap {
            buf.truncate(len as usize);
            unsafe { CloseHandle(dup) };
            return Some(String::from_utf16_lossy(&buf));
        }
        cap = (len as usize) + 1;
        if cap > 8192 {
            unsafe { CloseHandle(dup) };
            return None;
        }
    }
}

struct ProcessNames {
    system: Option<System>,
}

impl ProcessNames {
    fn new() -> Self {
        Self { system: None }
    }
    fn name_of(&mut self, pid: u32) -> String {
        let sys = self.system.get_or_insert_with(|| {
            let mut s = System::new();
            s.refresh_processes(ProcessesToUpdate::All, false);
            s
        });
        sys.process(sysinfo::Pid::from_u32(pid))
            .and_then(|p| p.name().to_str().map(str::to_owned))
            .unwrap_or_else(|| format!("pid-{pid}"))
    }
}

fn dedup(mut rows: Vec<LockedProcess>) -> Vec<LockedProcess> {
    let mut seen = std::collections::HashSet::new();
    rows.retain(|r| seen.insert((r.pid, r.path.clone())));
    rows
}

fn canonical(p: &Path) -> Option<PathBuf> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(p)
    };
    let comps: Vec<_> = abs.components().collect();
    let mut out: Vec<_> = Vec::new();
    for comp in comps {
        match comp {
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    Some(out.iter().collect())
}

fn normalize_target(s: &str) -> String {
    s.trim_start_matches("\\\\?\\")
        .trim_start_matches("\\??\\")
        .to_lowercase()
}

fn paths_equal(resolved: &str, target: &str) -> bool {
    normalize_target(resolved) == normalize_target(target)
}

/// Gracefully request the process to exit by posting `WM_CLOSE` (via `taskkill`
/// without `/F`, which asks top-level windows to close).
pub fn graceful_close(pid: u32) -> bool {
    let out = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// Forcefully terminate the process (`taskkill /F /PID`). Blocklist checks are
/// the caller's responsibility (see `kill_service`).
pub fn kill(pid: u32) -> bool {
    let out = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        assert!(locked_processes(&[]).is_empty());
    }

    #[test]
    fn paths_equal_is_case_insensitive_and_ignores_verbatim_prefix() {
        assert!(paths_equal(r"\\?\C:\Users\Foo\Test.txt", r"c:\users\foo\test.txt"));
        assert!(paths_equal(r"C:\A", r"c:\a"));
        assert!(!paths_equal(r"C:\A", r"C:\B"));
    }

    #[test]
    fn canonical_absolutizes_and_normalizes() {
        let cwd = std::env::current_dir().unwrap();
        let p = canonical(Path::new("./target")).unwrap();
        assert!(p.is_absolute());
        assert_eq!(p, cwd.join("target"));
    }

    #[test]
    fn dedup_removes_duplicate_pid_path_pairs() {
        let rows = vec![
            LockedProcess { pid: 1, name: "a".into(), path: PathBuf::from("x") },
            LockedProcess { pid: 1, name: "a".into(), path: PathBuf::from("x") },
            LockedProcess { pid: 2, name: "b".into(), path: PathBuf::from("x") },
        ];
        let d = dedup(rows);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn handle_entry_layout_is_sane() {
        assert!(core::mem::size_of::<SystemHandleTableEntryInfoEx>() >= 40);
    }

    // Live probe: enumerates the real system handle table, which needs a normal
    // interactive session. Hosted CI runners return no match, so this is
    // #[ignore]d there per Constitution Principle IV.
    #[test]
    #[ignore = "live probe: needs a real session with an enumerable handle table"]
    fn live_detects_own_locked_file_and_is_fast() {
        let dir = std::env::temp_dir();
        let path = dir.join("sweep-live-lock-test.bin");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("create lock file");
        std::io::Write::write_all(&mut file, b"data").unwrap();
        let start = std::time::Instant::now();
        let res = locked_processes(&[path.as_path()]);
        let elapsed = start.elapsed();
        drop(file);
        let _ = std::fs::remove_file(&path);
        let me = unsafe { GetCurrentProcessId() };
        assert!(res.iter().any(|r| r.pid == me), "expected to detect our own locked file, got {res:?}");
        assert!(elapsed.as_secs() < 60, "enumeration took too long: {elapsed:?}");
    }
}
