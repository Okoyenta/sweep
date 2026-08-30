//! Parsing of user-supplied cleaner rules from `sweep.toml` `[[category]]`.
//!
//! Each `[[category]]` entry declares a custom cache location that sweep should
//! discover and clean without a code change. Entries follow the same risk
//! visibility policy as built-in categories: `risk = "System"` is hidden unless
//! `sweep` is invoked with `--deep`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::models::{RiskLevel, RulePackCategory};
use crate::infra::exclusions::resolve_toml_path;

#[derive(Deserialize)]
struct RulePackEntry {
    id: String,
    roots: Vec<PathBuf>,
    #[serde(default = "default_risk")]
    risk: String,
    #[serde(default)]
    cleanup_command: Option<String>,
}

fn default_risk() -> String {
    "Safe".to_string()
}

#[derive(Deserialize, Default)]
struct SweepTomlDoc {
    #[serde(default)]
    category: Vec<RulePackEntry>,
}

/// Parse `[[category]]` rule-pack entries from `sweep.toml`.
///
/// Invalid entries (empty id, unknown risk) are skipped with a warning. A missing
/// or unparseable file yields an empty list (built-in categories remain active).
pub fn load_rule_packs(config_override: Option<&Path>) -> Vec<RulePackCategory> {
    let path = resolve_toml_path(config_override);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            // A path the user named explicitly is worth a message; the implicit
            // lookup finding nothing is the normal no-config case (FR-017).
            if config_override.is_some() {
                eprintln!(
                    "could not read rule pack '{}': {e} (using built-in categories)",
                    path.display()
                );
            }
            return Vec::new();
        }
    };
    parse_rule_packs(&content)
}

/// Parse rule-pack entries from TOML text, skipping invalid entries.
///
/// Split out from [`load_rule_packs`] so the validation rules are testable
/// without touching the filesystem.
fn parse_rule_packs(content: &str) -> Vec<RulePackCategory> {
    let doc = match toml::from_str::<SweepTomlDoc>(content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("sweep.toml parse error (rule packs ignored): {e}");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in doc.category {
        if entry.id.trim().is_empty() {
            eprintln!("skipping rule pack entry with empty id");
            continue;
        }
        if !seen.insert(entry.id.clone()) {
            eprintln!("skipping duplicate rule pack id: {}", entry.id);
            continue;
        }
        let risk = match entry.risk.to_ascii_lowercase().as_str() {
            "safe" => RiskLevel::Safe,
            "system" => RiskLevel::System,
            other => {
                eprintln!(
                    "rule pack '{}' has unknown risk '{}' (expected Safe|System); defaulting to Safe",
                    entry.id, other
                );
                RiskLevel::Safe
            }
        };
        out.push(RulePackCategory {
            id: entry.id,
            roots: entry.roots,
            risk,
            cleanup_command: entry.cleanup_command,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_category() {
        let packs = parse_rule_packs(
            r#"
[[category]]
id = "myapp-cache"
roots = ["C:/Users/me/AppData/Local/MyApp/Cache"]
risk = "Safe"
"#,
        );
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "myapp-cache");
        assert_eq!(packs[0].risk, RiskLevel::Safe);
        assert_eq!(packs[0].roots.len(), 1);
    }

    #[test]
    fn system_risk_is_preserved() {
        let packs = parse_rule_packs(
            r#"
[[category]]
id = "sys-thing"
roots = ["/tmp/x"]
risk = "System"
"#,
        );
        assert_eq!(packs[0].risk, RiskLevel::System);
    }

    #[test]
    fn unknown_risk_defaults_to_safe() {
        let packs = parse_rule_packs(
            r#"
[[category]]
id = "weird"
roots = ["/tmp/x"]
risk = "Dangerous"
"#,
        );
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].risk, RiskLevel::Safe);
    }

    #[test]
    fn empty_id_is_skipped() {
        let packs = parse_rule_packs(
            r#"
[[category]]
id = "  "
roots = ["/tmp/x"]
"#,
        );
        assert!(packs.is_empty());
    }

    #[test]
    fn duplicate_ids_are_skipped() {
        let packs = parse_rule_packs(
            r#"
[[category]]
id = "dup"
roots = ["/tmp/a"]

[[category]]
id = "dup"
roots = ["/tmp/b"]
"#,
        );
        assert_eq!(packs.len(), 1);
    }

    #[test]
    fn malformed_toml_yields_no_packs() {
        assert!(parse_rule_packs("this is not ][ valid").is_empty());
    }

    #[test]
    fn missing_explicit_path_falls_back_to_builtins() {
        let packs = load_rule_packs(Some(Path::new("no-such-rulepack-file.toml")));
        assert!(packs.is_empty());
    }
}
