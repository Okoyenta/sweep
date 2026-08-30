//! Loading of user exclusions from `sweep.toml`.
//!
//! Resolution order (highest precedence first):
//! 1. explicit `--config <path>` override
//! 2. `./sweep.toml` in the current working directory
//! 3. user config dir (`%LOCALAPPDATA%/sweep/sweep.toml` on Windows,
//!    `~/.config/sweep/sweep.toml` on Linux)
//!
//! Malformed or missing files yield an empty config (no exclusions applied)
//! rather than aborting the run.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::models::ExclusionConfig;
use crate::infra::paths::sweep_toml_path;

#[derive(Deserialize, Default)]
struct ExclusionsSection {
    #[serde(default)]
    paths: Vec<PathBuf>,
    #[serde(default)]
    category_ids: Vec<String>,
    #[serde(default)]
    globs: Vec<String>,
}

#[derive(Deserialize, Default)]
struct SweepTomlDoc {
    #[serde(default)]
    exclusions: ExclusionsSection,
}

/// Resolve the `sweep.toml` path to load, honoring the `--config` override and
/// falling back to CWD then the user config directory.
pub fn resolve_toml_path(config_override: Option<&Path>) -> PathBuf {
    if let Some(p) = config_override {
        return p.to_path_buf();
    }
    let cwd = PathBuf::from("sweep.toml");
    if cwd.exists() {
        return cwd;
    }
    sweep_toml_path()
}

/// Load the `[exclusions]` section of `sweep.toml`.
///
/// Returns an empty config (no exclusions) when the file is missing or
/// unparseable; parse errors are printed to stderr so the caller can proceed.
pub fn load_exclusions(config_override: Option<&Path>) -> ExclusionConfig {
    let path = resolve_toml_path(config_override);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            // Only complain about a path the user named explicitly; no config at
            // all is the normal case and must stay silent (FR-017).
            if config_override.is_some() {
                eprintln!(
                    "could not read config '{}': {e} (no exclusions applied)",
                    path.display()
                );
            }
            return ExclusionConfig::default();
        }
    };
    parse_exclusions(&content)
}

/// Parse the `[exclusions]` section from TOML text.
///
/// Split out from [`load_exclusions`] so the malformed-input contract is
/// testable without touching the filesystem.
fn parse_exclusions(content: &str) -> ExclusionConfig {
    match toml::from_str::<SweepTomlDoc>(content) {
        Ok(parsed) => ExclusionConfig {
            paths: parsed.exclusions.paths,
            category_ids: parsed.exclusions.category_ids,
            globs: parsed.exclusions.globs,
        },
        Err(e) => {
            eprintln!("sweep.toml parse error (exclusions ignored): {e}");
            ExclusionConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_three_exclusion_kinds() {
        let cfg = parse_exclusions(
            r#"
[exclusions]
paths = ["C:/Games/Cache"]
category_ids = ["dev-pnpm"]
globs = ["**/node_modules/**"]
"#,
        );
        assert_eq!(cfg.paths.len(), 1);
        assert_eq!(cfg.category_ids, vec!["dev-pnpm".to_string()]);
        assert_eq!(cfg.globs, vec!["**/node_modules/**".to_string()]);
    }

    #[test]
    fn malformed_toml_yields_empty_config() {
        // FR-017: a bad config must degrade to "no exclusions", never panic.
        let cfg = parse_exclusions("[exclusions\npaths = broken ][");
        assert!(cfg.is_empty());
    }

    #[test]
    fn absent_section_yields_empty_config() {
        let cfg = parse_exclusions("[[category]]\nid = \"x\"\nroots = []\n");
        assert!(cfg.is_empty());
    }

    #[test]
    fn missing_file_yields_empty_config() {
        let cfg = load_exclusions(Some(Path::new("no-such-sweep-config.toml")));
        assert!(cfg.is_empty());
    }
}
