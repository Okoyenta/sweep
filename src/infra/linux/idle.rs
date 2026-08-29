use crate::domain::models::IdleProbeResult;

pub struct IdleProbe;

impl IdleProbe {
    pub fn new() -> Self {
        Self
    }

    pub fn poll(&self) -> anyhow::Result<IdleProbeResult> {
        Ok(IdleProbeResult {
            idle_seconds: 0,
            foreground_title: String::new(),
        })
    }
}
