//! Applying user exclusions to discovered clean categories.
//!
//! Exclusions are honored across every cleaning path (diagnose, clean, guard):
//! a category whose id is excluded, or whose roots fall under an excluded path
//! or glob, is dropped *before* size calculation so excluded space is never
//! counted or touched.

use std::path::{Path, PathBuf};

use crate::domain::models::{CleanCategory, ExclusionConfig, RiskLevel, RulePackCategory};

/// Filter `categories` down to those not excluded by `excl`.
///
/// Returns the surviving categories and the count of excluded ones (for logging).
pub fn apply_exclusions(
    categories: &[CleanCategory],
    excl: &ExclusionConfig,
) -> (Vec<CleanCategory>, usize) {
    if excl.is_empty() {
        return (categories.to_vec(), 0);
    }
    let mut kept = Vec::new();
    let mut excluded = 0;
    for cat in categories {
        if is_excluded(cat, excl) {
            excluded += 1;
            continue;
        }
        kept.push(cat.clone());
    }
    (kept, excluded)
}

fn is_excluded(cat: &CleanCategory, excl: &ExclusionConfig) -> bool {
    if excl.category_ids.iter().any(|id| id == &cat.id) {
        return true;
    }
    cat.roots.iter().any(|root| is_path_excluded(root, excl))
}

/// True when `path` is covered by an excluded path or glob.
///
/// Used both to drop whole categories and to prune individual candidate items
/// before their size is measured (FR-005).
pub fn is_path_excluded(path: &Path, excl: &ExclusionConfig) -> bool {
    if excl.paths.is_empty() && excl.globs.is_empty() {
        return false;
    }
    // A category root that *contains* an excluded path still has excluded
    // children, so containment is checked in both directions.
    if excl
        .paths
        .iter()
        .any(|p| path_under(p, path) || path_under(path, p))
    {
        return true;
    }
    let text = path.to_string_lossy();
    excl.globs.iter().any(|g| glob_match(g, &text))
}

/// True when `child` is the same as or nested under `parent`.
///
/// Comparison is case-insensitive on Windows, where paths are case-preserving
/// but not case-sensitive.
fn path_under(parent: &Path, child: &Path) -> bool {
    if child == parent || child.starts_with(parent) {
        return true;
    }
    if cfg!(windows) {
        let p = normalize_sep(&parent.to_string_lossy().to_lowercase());
        let c = normalize_sep(&child.to_string_lossy().to_lowercase());
        return c == p || c.starts_with(&format!("{}/", p.trim_end_matches('/')));
    }
    false
}

/// Rewrite backslashes to `/` so a single glob works on both platforms.
fn normalize_sep(s: &str) -> String {
    s.replace('\\', "/")
}

/// Very small glob matcher supporting `*` (any chars except separator) and
/// `**` (any chars including separators).
///
/// Both pattern and text are lowercased and separator-normalized first, so
/// `**/node_modules/**` matches a Windows path (data-model.md: globs are
/// matched case-insensitively).
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = normalize_sep(&pattern.to_lowercase());
    let text = normalize_sep(&text.to_lowercase());
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_impl(&p, 0, &t, 0)
}

fn glob_match_impl(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
    if pi == p.len() {
        return ti == t.len();
    }
    if p[pi] == '*' {
        // Double-star: consumes separators too.
        if pi + 1 < p.len() && p[pi + 1] == '*' {
            // Try to consume 0..=n chars (including separators).
            for skip in 0..=t.len() - ti {
                if glob_match_impl(p, pi + 2, t, ti + skip) {
                    return true;
                }
            }
            return false;
        }
        // Single star: consume until next separator or end.
        for skip in 0..=(t.len() - ti) {
            // Text is separator-normalized before matching, so '/' is the only
            // boundary a single `*` must not cross.
            if skip > 0 && t[ti + skip - 1] == '/' {
                break;
            }
            if glob_match_impl(p, pi + 1, t, ti + skip) {
                return true;
            }
        }
        return false;
    }
    if ti < t.len() && p[pi] == t[ti] {
        return glob_match_impl(p, pi + 1, t, ti + 1);
    }
    false
}

/// Build an `ExclusionConfig` plus the custom rule-pack categories to merge into
/// discovery. This is the single entry point used by diagnose/clean/guard so the
/// exclusion + rule-pack policy stays consistent everywhere.
pub fn load_policy(config_override: Option<&Path>) -> (ExclusionConfig, Vec<RulePackCategory>) {
    load_policy_with_rules(config_override, None)
}

/// Like [`load_policy`], but also merges an extra rule-pack file supplied via
/// `--rules <path>` (research R9). Entries from the extra pack are appended
/// after the `sweep.toml` ones; duplicate ids are dropped by
/// [`rule_packs_to_categories`].
pub fn load_policy_with_rules(
    config_override: Option<&Path>,
    rules_override: Option<&Path>,
) -> (ExclusionConfig, Vec<RulePackCategory>) {
    let excl = crate::infra::exclusions::load_exclusions(config_override);
    let mut packs = crate::infra::rulepack::load_rule_packs(config_override);
    if let Some(rules) = rules_override {
        packs.extend(crate::infra::rulepack::load_rule_packs(Some(rules)));
    }
    (excl, packs)
}

/// Expand `%VAR%` (Windows) and `$VAR` (Linux) references in a rule-pack root.
///
/// An unset variable leaves the token untouched; the resulting path simply will
/// not exist and is skipped like any other missing root.
pub fn expand_env(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy().into_owned();
    let mut out = String::with_capacity(raw.len());
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                match std::env::var(&name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&format!("%{name}%")),
                }
                i += end + 2;
                continue;
            }
        }
        if chars[i] == '$' {
            let end = chars[i + 1..]
                .iter()
                .position(|c| !c.is_ascii_alphanumeric() && *c != '_')
                .map(|p| i + 1 + p)
                .unwrap_or(chars.len());
            if end > i + 1 {
                let name: String = chars[i + 1..end].iter().collect();
                match std::env::var(&name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&format!("${name}")),
                }
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    PathBuf::from(out)
}

/// Convert user rule packs into discoverable [`CleanCategory`] entries.
///
/// Applies the same visibility policy as built-in categories: `risk = "System"`
/// entries are only returned when `deep` is set (FR-016). Entries whose id
/// collides with a built-in (or an earlier pack) are skipped with a warning, and
/// entries with no existing root are dropped silently, exactly like built-in
/// discovery.
pub fn rule_packs_to_categories(
    packs: &[RulePackCategory],
    builtin: &[CleanCategory],
    deep: bool,
) -> Vec<CleanCategory> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = builtin.iter().map(|c| c.id.clone()).collect();
    for pack in packs {
        if pack.risk == RiskLevel::System && !deep {
            continue;
        }
        if seen.iter().any(|id| id == &pack.id) {
            eprintln!(
                "rule pack '{}' collides with an existing category id; skipping",
                pack.id
            );
            continue;
        }
        let roots: Vec<PathBuf> = pack
            .roots
            .iter()
            .map(|r| expand_env(r))
            .filter(|r| r.exists())
            .collect();
        if roots.is_empty() {
            continue;
        }
        seen.push(pack.id.clone());
        out.push(CleanCategory {
            id: pack.id.clone(),
            title: pack.id.clone(),
            roots,
            risk: pack.risk,
            cleanup_command: pack.cleanup_command.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::domain::models::RiskLevel;

    fn cat(id: &str, roots: &[&str]) -> CleanCategory {
        CleanCategory {
            id: id.into(),
            title: id.into(),
            roots: roots.iter().map(PathBuf::from).collect(),
            risk: RiskLevel::Safe,
            cleanup_command: None,
        }
    }

    #[test]
    fn empty_exclusion_keeps_all() {
        let cats = vec![cat("a", &["C:\\a"])];
        let (kept, n) = apply_exclusions(&cats, &ExclusionConfig::default());
        assert_eq!(kept.len(), 1);
        assert_eq!(n, 0);
    }

    #[test]
    fn excludes_by_category_id() {
        let cats = vec![cat("dev-pnpm", &["C:\\pnpm"])];
        let excl = ExclusionConfig {
            category_ids: vec!["dev-pnpm".into()],
            ..Default::default()
        };
        let (kept, n) = apply_exclusions(&cats, &excl);
        assert_eq!(kept.len(), 0);
        assert_eq!(n, 1);
    }

    #[test]
    fn excludes_by_path_prefix() {
        let cats = vec![cat("user-temp", &["C:\\Users\\me\\AppData\\Local\\Temp"])];
        let excl = ExclusionConfig {
            paths: vec![PathBuf::from("C:\\Users\\me\\AppData\\Local\\Temp")],
            ..Default::default()
        };
        let (kept, n) = apply_exclusions(&cats, &excl);
        assert_eq!(kept.len(), 0);
        assert_eq!(n, 1);
    }

    #[test]
    fn glob_star_matches_nested_node_modules() {
        assert!(glob_match("**/node_modules/**", "C:\\proj\\node_modules\\x"));
        assert!(glob_match("**/node_modules", "C:\\proj\\node_modules"));
        assert!(!glob_match("**/node_modules/**", "C:\\proj\\src\\x"));
    }

    #[test]
    fn glob_matching_is_case_insensitive() {
        assert!(glob_match("**/Node_Modules/**", r"C:\proj\node_modules\x"));
        assert!(glob_match("**/.Cache/**", "/home/me/.CACHE/thing"));
        // A literal segment must still match a whole segment: `cache` does not
        // match the `.cache` directory.
        assert!(!glob_match("**/cache/**", "/home/me/.CACHE/thing"));
    }

    // Backslash paths only behave as paths on Windows; on Linux `C:\Games\Cache`
    // is one filename component, so each platform gets its own fixture.
    #[cfg(windows)]
    #[test]
    fn is_path_excluded_matches_prefix_and_glob() {
        let excl = ExclusionConfig {
            paths: vec![PathBuf::from(r"C:\Games")],
            globs: vec!["**/node_modules/**".into()],
            ..Default::default()
        };
        assert!(is_path_excluded(Path::new(r"C:\Games\Cache"), &excl));
        assert!(is_path_excluded(Path::new(r"C:\proj\node_modules\x"), &excl));
        assert!(!is_path_excluded(Path::new(r"C:\proj\src"), &excl));
    }

    #[cfg(not(windows))]
    #[test]
    fn is_path_excluded_matches_prefix_and_glob() {
        let excl = ExclusionConfig {
            paths: vec![PathBuf::from("/games")],
            globs: vec!["**/node_modules/**".into()],
            ..Default::default()
        };
        assert!(is_path_excluded(Path::new("/games/cache"), &excl));
        assert!(is_path_excluded(Path::new("/proj/node_modules/x"), &excl));
        assert!(!is_path_excluded(Path::new("/proj/src"), &excl));
    }

    #[test]
    fn empty_config_excludes_nothing() {
        let excl = ExclusionConfig::default();
        assert!(!is_path_excluded(Path::new("/anything"), &excl));
        assert!(!is_path_excluded(Path::new(r"C:\anything"), &excl));
    }

    #[test]
    fn system_rule_packs_are_hidden_unless_deep() {
        let pack = RulePackCategory {
            id: "sys".into(),
            roots: vec![std::env::temp_dir()],
            risk: RiskLevel::System,
            cleanup_command: None,
        };
        assert!(rule_packs_to_categories(&[pack.clone()], &[], false).is_empty());
        assert_eq!(rule_packs_to_categories(&[pack], &[], true).len(), 1);
    }

    #[test]
    fn rule_pack_colliding_with_builtin_is_skipped() {
        let builtin = vec![cat("user-temp", &[r"C:\temp"])];
        let pack = RulePackCategory {
            id: "user-temp".into(),
            roots: vec![std::env::temp_dir()],
            risk: RiskLevel::Safe,
            cleanup_command: None,
        };
        assert!(rule_packs_to_categories(&[pack], &builtin, false).is_empty());
    }

    #[test]
    fn rule_pack_with_no_existing_root_is_dropped() {
        let pack = RulePackCategory {
            id: "ghost".into(),
            roots: vec![PathBuf::from("/definitely/not/here/xyz")],
            risk: RiskLevel::Safe,
            cleanup_command: None,
        };
        assert!(rule_packs_to_categories(&[pack], &[], false).is_empty());
    }

    #[test]
    fn expand_env_substitutes_known_vars_and_keeps_unknown() {
        unsafe { std::env::set_var("SWEEP_TEST_ROOT", "/opt/base") };
        assert_eq!(
            expand_env(Path::new("$SWEEP_TEST_ROOT/cache")),
            PathBuf::from("/opt/base/cache")
        );
        assert_eq!(
            expand_env(Path::new("%SWEEP_TEST_ROOT%/cache")),
            PathBuf::from("/opt/base/cache")
        );
        assert_eq!(
            expand_env(Path::new("%SWEEP_NOT_SET_VAR%/x")),
            PathBuf::from("%SWEEP_NOT_SET_VAR%/x")
        );
        unsafe { std::env::remove_var("SWEEP_TEST_ROOT") };
    }
}
