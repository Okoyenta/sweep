use anyhow::Context;

pub const TASK_NAME: &str = "SweepIndex";
pub const CRON_MARKER: &str = "#sweep-index";

fn exe_path() -> anyhow::Result<std::path::PathBuf> {
    std::env::current_exe().context("resolving sweep executable path")
}

#[cfg(windows)]
pub mod windows_impl {
    use super::*;

    pub fn create_args(exe: &std::path::Path) -> Vec<String> {
        vec![
            "/Create".into(),
            "/F".into(),
            "/TN".into(),
            TASK_NAME.into(),
            "/SC".into(),
            "DAILY".into(),
            "/ST".into(),
            "03:00".into(),
            "/TR".into(),
            format!("\"{}\" index", exe.display()),
        ]
    }

    pub fn delete_args() -> Vec<String> {
        vec!["/Delete".into(), "/F".into(), "/TN".into(), TASK_NAME.into()]
    }

    pub fn query_args() -> Vec<String> {
        vec!["/Query".into(), "/TN".into(), TASK_NAME.into()]
    }

    pub fn run(args: &[String]) -> anyhow::Result<bool> {
        let out = std::process::Command::new("schtasks")
            .args(args)
            .output()
            .map_err(|e| anyhow::anyhow!("running schtasks failed: {e}"))?;
        Ok(out.status.success())
    }
}

#[cfg(not(windows))]
pub mod unix_impl {
    use super::*;

    pub fn cron_line(exe: &std::path::Path) -> String {
        format!("@daily {} index {CRON_MARKER}", exe.display())
    }

    pub fn install() -> anyhow::Result<()> {
        let line = cron_line(&exe_path()?);
        let existing = std::process::Command::new("crontab")
            .arg("-l")
            .output()
            .map_err(|e| anyhow::anyhow!("reading crontab failed (installed?): {e}"))?;
        let mut body = String::from_utf8_lossy(&existing.stdout).into_owned();
        if body.contains(CRON_MARKER) {
            println!("cron entry already present");
            return Ok(());
        }
        if !body.ends_with('\n') && !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&line);
        body.push('\n');
        run_crontab(&body)
    }

    pub fn remove() -> anyhow::Result<()> {
        let existing = std::process::Command::new("crontab")
            .arg("-l")
            .output()
            .map_err(|e| anyhow::anyhow!("reading crontab failed: {e}"))?;
        let stdout = String::from_utf8_lossy(&existing.stdout);
        let kept: Vec<&str> = stdout
            .lines()
            .filter(|l| !l.contains(CRON_MARKER))
            .collect();
        run_crontab(&format!("{}\n", kept.join("\n")))
    }

    pub fn is_installed() -> anyhow::Result<bool> {
        let out = std::process::Command::new("sh")
            .args(["-c", "crontab -l 2>/dev/null || true"])
            .output()
            .map_err(|e| anyhow::anyhow!("crontab failed: {e}"))?;
        Ok(String::from_utf8_lossy(&out.stdout).contains(CRON_MARKER))
    }

    fn run_crontab(body: &str) -> anyhow::Result<()> {
        use std::io::Write;
        let mut child = std::process::Command::new("crontab")
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("spawning crontab failed: {e}"))?;
        child
            .stdin
            .as_mut()
            .expect("piped stdin")
            .write_all(body.as_bytes())?;
        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("crontab rejected the new table")
        }
    }
}

#[cfg(windows)]
pub fn install() -> anyhow::Result<()> {
    let args = windows_impl::create_args(&exe_path()?);
    if windows_impl::run(&args)? {
        println!("scheduled task '{TASK_NAME}' installed (daily at 03:00)");
        Ok(())
    } else {
        anyhow::bail!("schtasks /Create failed")
    }
}

#[cfg(windows)]
pub fn remove() -> anyhow::Result<()> {
    if windows_impl::run(&windows_impl::delete_args())? {
        println!("scheduled task '{TASK_NAME}' removed");
        Ok(())
    } else {
        anyhow::bail!("task not found or deletion refused")
    }
}

#[cfg(windows)]
pub fn is_installed() -> anyhow::Result<bool> {
    Ok(windows_impl::run(&windows_impl::query_args())?)
}

#[cfg(not(windows))]
pub use unix_impl::{install, is_installed, remove};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn schtasks_args_are_well_formed() {
        let args = windows_impl::create_args(std::path::Path::new("C:\\tools\\sweep.exe"));
        assert_eq!(args[0], "/Create");
        assert!(args.windows(2).any(|w| w[0] == "/TR"
            && w[1].contains("C:\\tools\\sweep.exe")
            && w[1].ends_with("index")));
    }

    #[test]
    #[cfg(not(windows))]
    fn cron_line_carries_marker_and_index() {
        let line = unix_impl::cron_line(std::path::Path::new("/usr/bin/sweep"));
        assert!(line.starts_with("@daily /usr/bin/sweep index"));
        assert!(line.ends_with(CRON_MARKER));
    }
}
