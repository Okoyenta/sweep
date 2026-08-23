#[cfg(windows)]
#[test]
#[ignore = "live probe: needs a real user profile with UserAssist history"]
fn live_userassist_probe_returns_entries() {
    use sweep::domain::traits::UsageProbe;
    let probe = sweep::infra::win::userassist::UserAssistProbe::new();
    let out = probe.probe().expect("probe should not hard-fail");
    println!("userassist exe entries: {}", out.len());
    assert!(out.len() > 0, "expected some .exe entries from live UserAssist");
}

#[cfg(not(windows))]
#[test]
fn placeholder() {}
