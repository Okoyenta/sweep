#[cfg(windows)]
pub fn send_toast(title: &str, body: &str) -> anyhow::Result<()> {
    let ps_script = format!(
        r#"
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
        [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] | Out-Null

        $template = @"
        <toast>
            <visual>
                <binding template="ToastText02">
                    <text id="1">{title}</text>
                    <text id="2">{body}</text>
                </binding>
            </visual>
        </toast>
        "@

        $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $xml.LoadXml($template)
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Sweep").Show($toast)
        "#,
        title = title.replace('"', "`\""),
        body = body.replace('"', "`\""),
    );

    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Ok(()), // graceful no-op on failure
    }
}

#[cfg(not(windows))]
pub fn send_toast(_title: &str, _body: &str) -> anyhow::Result<()> {
    Ok(())
}

/// Shows a blocking Yes/No confirmation dialog (Windows MsgBox via
/// PowerShell) listing the apps to be killed. Returns `true` only if the
/// user explicitly clicked Yes. On Linux or if the dialog cannot be shown,
/// falls back to the plain-text `confirm` prompt.
#[cfg(windows)]
pub fn confirm_kill(apps: &[crate::domain::models::LockedProcess]) -> bool {
    let mut seen = std::collections::HashSet::new();
    let lines: Vec<String> = apps
        .iter()
        .filter(|a| seen.insert(a.pid))
        .map(|a| format!("{} (PID {})", a.name, a.pid))
        .collect();
    let list = lines.join("\n");
    let script = format!(
        r#"
        Add-Type -AssemblyName System.Windows.Forms
        $r = [System.Windows.Forms.MessageBox]::Show(
            'The following apps hold files sweep wants to clean. Kill them?' +
            [char]10 + [char]10 +
            '{list}' + [char]10 + [char]10 +
            'Unsaved work may be lost.',
            'Sweep - close apps',
            [System.Windows.Forms.MessageBoxButtons]::YesNo,
            [System.Windows.Forms.MessageBoxIcon]::Warning
        )
        if ($r -eq [System.Windows.Forms.DialogResult]::Yes) {{ 'yes' }} else {{ 'no' }}
        "#,
        list = list.replace("'", "''").replace("\n", "' + [char]10 + '"),
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.trim().eq_ignore_ascii_case("yes")
        }
        Err(_) => crate::ui::apps::confirm(&format!(
            "kill {} app(s) holding locked files and clean their caches?",
            apps.len()
        )),
    }
}

#[cfg(not(windows))]
pub fn confirm_kill(apps: &[crate::domain::models::LockedProcess]) -> bool {
    crate::ui::apps::confirm(&format!(
        "kill {} app(s) holding locked files and clean their caches?",
        apps.len()
    ))
}

pub fn print_guard_cycle(
    ram_pct: f64,
    disk_free_gb: f64,
    action: &str,
    freed: Option<u64>,
) {
    println!(
        "[guard] ram {:.0}% | disk {:.1} GB free | action: {}{}",
        ram_pct * 100.0,
        disk_free_gb,
        action,
        freed
            .map(|b| format!(" | freed {}", crate::ui::status::fmt(b)))
            .unwrap_or_default(),
    );
}

pub fn print_guard_idle() {
    println!("[guard] all clear — no action needed");
}
