//! Guards the one call-site rule that the type system cannot fully express.
//!
//! `ensure_reserve()` allocates the full 512 MiB reserve without consulting free
//! space, so calling it from a command that reclaims nothing hands back space a
//! `clean` had just freed — the bug this file exists to prevent from returning.
//! It is private to `infra::paths` for that reason; this scan is the backstop for
//! someone re-exporting it, or for a future module adding its own wrapper.

use std::fs;
use std::path::{Path, PathBuf};

/// The only file permitted to name `ensure_reserve`: it holds the definition, the
/// doc comment explaining why it is private, and its own unit tests.
const DEFINITION_SITE: &str = "src/infra/paths.rs";

/// This file, which necessarily contains the name it searches for.
const SELF: &str = "tests/reserve_call_sites.rs";

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn no_call_site_invokes_bare_ensure_reserve() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();

    for dir in ["src", "tests"] {
        for file in rust_sources(&manifest.join(dir)) {
            let rel = file
                .strip_prefix(&manifest)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            if rel == DEFINITION_SITE || rel == SELF {
                continue;
            }
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            if text.contains("ensure_reserve") {
                offenders.push(rel);
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these files reference `ensure_reserve`, which ignores free space and so \
         re-creates the reserve on disks that cannot spare it. Call \
         `try_recreate_reserve()` instead, which gates on \
         RECREATION_THRESHOLD_BYTES:\n  {}",
        offenders.join("\n  ")
    );
}
