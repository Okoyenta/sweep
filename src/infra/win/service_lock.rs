use crate::domain::models::ServiceGuardState;

#[cfg(windows)]
pub struct ServiceGuard {
    states: Vec<ServiceGuardState>,
}

#[cfg(windows)]
impl ServiceGuard {
    pub fn new(services: &[&str]) -> anyhow::Result<Self> {
        let mut states = Vec::new();
        for name in services {
            let was_running = is_service_running(name);
            if was_running {
                stop_service(name)?;
            }
            states.push(ServiceGuardState {
                name: name.to_string(),
                was_running,
            });
        }
        Ok(Self { states })
    }
}

#[cfg(windows)]
impl Drop for ServiceGuard {
    fn drop(&mut self) {
        for state in &self.states {
            if state.was_running {
                let _ = start_service(&state.name);
            }
        }
    }
}

#[cfg(windows)]
fn is_service_running(name: &str) -> bool {
    let output = std::process::Command::new("sc")
        .args(["query", name])
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains("RUNNING")
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn stop_service(name: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("net")
        .args(["stop", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        anyhow::bail!("failed to stop service '{name}'");
    }
    Ok(())
}

#[cfg(windows)]
fn start_service(name: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("net")
        .args(["start", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        anyhow::bail!("failed to start service '{name}'");
    }
    Ok(())
}

#[cfg(not(windows))]
pub struct ServiceGuard;

#[cfg(not(windows))]
impl ServiceGuard {
    pub fn new(_services: &[&str]) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_guard_builds_with_empty_list() {
        let guard = ServiceGuard::new(&[]).unwrap();
        drop(guard);
    }
}
