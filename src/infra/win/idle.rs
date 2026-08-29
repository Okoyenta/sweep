use crate::domain::models::IdleProbeResult;

pub struct IdleProbe;

impl IdleProbe {
    pub fn new() -> Self {
        Self
    }

    pub fn poll(&self) -> anyhow::Result<IdleProbeResult> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                GetLastInputInfo, LASTINPUTINFO,
            };
            use windows_sys::Win32::System::SystemInformation::GetTickCount;

            unsafe {
                let mut lii: LASTINPUTINFO = std::mem::zeroed();
                lii.cbSize = std::mem::size_of::<LASTINPUTINFO>() as u32;
                if GetLastInputInfo(&mut lii) == 0 {
                    return Ok(IdleProbeResult {
                        idle_seconds: 0,
                        foreground_title: String::new(),
                    });
                }
                let tick = GetTickCount();
                let elapsed_ms = tick.saturating_sub(lii.dwTime);
                let idle_seconds = elapsed_ms / 1000;
                Ok(IdleProbeResult {
                    idle_seconds: idle_seconds as u64,
                    foreground_title: String::new(),
                })
            }
        }
        #[cfg(not(windows))]
        {
            Ok(IdleProbeResult {
                idle_seconds: 0,
                foreground_title: String::new(),
            })
        }
    }
}
