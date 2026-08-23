use std::collections::HashSet;
use std::path::PathBuf;

use crate::domain::models::InstalledApp;
use crate::domain::traits::AppInventory;

pub struct DesktopFileInventory {
    dirs: Vec<PathBuf>,
}

impl DesktopFileInventory {
    pub fn new() -> Self {
        let mut dirs = vec![PathBuf::from("/usr/share/applications")];
        if let Some(home) = std::env::var_os("HOME") {
            let mut user_dir = PathBuf::from(home);
            user_dir.extend([".local", "share", "applications"]);
            dirs.push(user_dir);
        }
        Self { dirs }
    }

    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

impl Default for DesktopFileInventory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ParsedDesktopEntry {
    pub name: String,
    pub version: String,
    pub is_application: bool,
    pub hidden_or_undisplayable: bool,
    pub exec: Option<String>,
}

/// parses only the first `[Desktop Entry]` section; everything else
/// (e.g. `[Desktop Action ...]`) is ignored
pub fn parse_desktop_entry(content: &str) -> ParsedDesktopEntry {
    let mut name = String::new();
    let mut version = String::new();
    let mut exec: Option<String> = None;
    let mut is_application = false;
    let mut hidden = false;
    let mut in_main_section = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_main_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_main_section || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = value.trim().to_string(),
            "Version" => version = value.trim().to_string(),
            "Exec" => exec = Some(value.trim().to_string()),
            "Type" => is_application = value.trim() == "Application",
            "NoDisplay" | "Hidden" => {
                if value.trim().eq_ignore_ascii_case("true") {
                    hidden = true;
                }
            }
            _ => {}
        }
    }

    ParsedDesktopEntry {
        name,
        version,
        is_application,
        hidden_or_undisplayable: hidden,
        exec,
    }
}

impl AppInventory for DesktopFileInventory {
    fn installed_apps(&self) -> anyhow::Result<Vec<InstalledApp>> {
        let mut apps: Vec<InstalledApp> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for dir in &self.dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let parsed = parse_desktop_entry(&content);
                if !parsed.is_application || parsed.hidden_or_undisplayable || parsed.name.is_empty()
                {
                    continue;
                }
                let dedup_key = format!("{}|{}", parsed.name.to_lowercase(), parsed.version);
                if seen.insert(dedup_key) {
                    apps.push(InstalledApp {
                        name: parsed.name,
                        version: parsed.version,
                        publisher: String::new(),
                        install_location: None,
                        uninstall_command: None,
                        size_bytes: None,
                        last_run_unix: None,
                    });
                }
            }
        }

        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(apps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox Web Browser
GenericName=Web Browser
Comment=Browse the Web
Exec=firefox %u
Version=1.0
NoDisplay=false

[Desktop Action NewWindow]
Name=Open a New Window
Exec=firefox --new-window %u
";

    #[test]
    fn parses_main_entry_and_ignores_actions() {
        let p = parse_desktop_entry(SAMPLE);
        assert_eq!(p.name, "Firefox Web Browser");
        assert_eq!(p.version, "1.0");
        assert!(p.is_application);
        assert!(!p.hidden_or_undisplayable);
        assert_eq!(p.exec.as_deref(), Some("firefox %u"));
    }

    #[test]
    fn flags_hidden_entries() {
        let p = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=X\nExec=x\nHidden=true\n",
        );
        assert!(p.hidden_or_undisplayable);
    }

    #[test]
    fn rejects_non_applications() {
        let p = parse_desktop_entry("[Desktop Entry]\nType=Link\nName=Y\n");
        assert!(!p.is_application);
    }

    #[test]
    fn inventory_reads_dirs_dedups_and_sorts() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("sweep-desktop-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("b.desktop"), "[Desktop Entry]\nType=Application\nName=Bravo\n").unwrap();
        fs::write(
            base.join("a1.desktop"),
            "[Desktop Entry]\nType=Application\nName=Alpha\n",
        )
        .unwrap();
        fs::write(
            base.join("a2.desktop"),
            "[Desktop Entry]\nType=Application\nName=alpha\n",
        )
        .unwrap();
        fs::write(base.join("hidden.desktop"), "[Desktop Entry]\nType=Application\nName=Zed\nNoDisplay=true\n").unwrap();

        let inv = DesktopFileInventory::with_dirs(vec![base.clone()]);
        let apps = inv.installed_apps().unwrap();
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Bravo"]);
        let _ = fs::remove_dir_all(&base);
    }
}
