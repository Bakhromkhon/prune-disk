use crate::model::Rule;
use crate::path_policy::{ValidatedPath, parse_path_pattern};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

/// Built-in rules are compiled into the binary so a fresh install has a useful
/// rule set without any network access or extra files.
pub const BUILTIN_RULE_SOURCES: &[(&str, &str)] = &[
    (
        "xcode-derived-data.toml",
        include_str!("../../../rules/macos/xcode-derived-data.toml"),
    ),
    (
        "xcode-archives.toml",
        include_str!("../../../rules/macos/xcode-archives.toml"),
    ),
    (
        "xcode-device-support.toml",
        include_str!("../../../rules/macos/xcode-device-support.toml"),
    ),
    (
        "homebrew-cache.toml",
        include_str!("../../../rules/macos/homebrew-cache.toml"),
    ),
    (
        "chrome-cache.toml",
        include_str!("../../../rules/macos/chrome-cache.toml"),
    ),
    (
        "safari-cache.toml",
        include_str!("../../../rules/macos/safari-cache.toml"),
    ),
    (
        "firefox-cache.toml",
        include_str!("../../../rules/macos/firefox-cache.toml"),
    ),
    (
        "trash.toml",
        include_str!("../../../rules/macos/trash.toml"),
    ),
    ("logs.toml", include_str!("../../../rules/macos/logs.toml")),
    (
        "ios-backups.toml",
        include_str!("../../../rules/macos/ios-backups.toml"),
    ),
    (
        "docker-desktop.toml",
        include_str!("../../../rules/macos/docker-desktop.toml"),
    ),
    (
        "npm-cache.toml",
        include_str!("../../../rules/common/npm-cache.toml"),
    ),
    (
        "pnpm-store.toml",
        include_str!("../../../rules/common/pnpm-store.toml"),
    ),
    (
        "yarn-cache.toml",
        include_str!("../../../rules/common/yarn-cache.toml"),
    ),
    (
        "cargo-cache.toml",
        include_str!("../../../rules/common/cargo-cache.toml"),
    ),
    (
        "pip-cache.toml",
        include_str!("../../../rules/common/pip-cache.toml"),
    ),
    (
        "go-build-cache.toml",
        include_str!("../../../rules/common/go-build-cache.toml"),
    ),
];

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("failed to parse rule {name}: {source}")]
    Parse {
        name: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to read rule file {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read rules directory {path}: {source}")]
    ReadDir {
        path: String,
        #[source]
        source: walkdir::Error,
    },
    #[error("rule {rule} has no paths")]
    NoPaths { rule: String },
    #[error("rule {rule} has an invalid OS: {os}")]
    InvalidOs { rule: String, os: String },
    #[error("rule {rule} has an unsafe path pattern: {path} ({reason})")]
    UnsafePath {
        rule: String,
        path: String,
        reason: String,
    },
    #[error("duplicate rule id: {0}")]
    DuplicateId(String),
    #[error("invalid rule id: {0}")]
    InvalidId(String),
    #[error("invalid trusted root: {0}")]
    InvalidRoot(String),
}

/// A validated, immutable scan capability. There is deliberately no Deserialize,
/// mutable accessor, or conversion from an unchecked Vec<Rule>.
///
/// ```compile_fail
/// use prune_core::{ScanOptions, Rule};
/// let options = ScanOptions { rules: Vec::<Rule>::new(), ..Default::default() };
/// ```
#[derive(Debug, Clone)]
pub struct ValidatedRuleSet {
    pub(crate) root: Option<PathBuf>,
    pub(crate) entries: Vec<ValidatedRule>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedRule {
    pub(crate) rule: Rule,
    pub(crate) paths: Vec<ValidatedPath>,
}

impl ValidatedRuleSet {
    /// Validate for the current user's home, never an arbitrary filesystem root.
    pub fn new(rules: Vec<Rule>) -> Result<Self, RuleError> {
        let home = dirs::home_dir()
            .ok_or_else(|| RuleError::InvalidRoot("home directory is unknown".into()))?;
        Self::for_trusted_root(rules, home)
    }

    /// Explicitly trust a directory as the scan boundary (e.g. an isolated test
    /// fixture). The filesystem/drive root is never accepted. The caller must
    /// own this trust decision; do not derive it from untrusted rule content.
    pub fn for_trusted_root(rules: Vec<Rule>, root: impl AsRef<Path>) -> Result<Self, RuleError> {
        let supplied = root.as_ref();
        if !supplied.is_absolute() {
            return Err(RuleError::InvalidRoot("root must be absolute".into()));
        }
        let root = fs::canonicalize(supplied)
            .map_err(|error| RuleError::InvalidRoot(error.to_string()))?;
        if root.parent().is_none() || !root.is_dir() {
            return Err(RuleError::InvalidRoot(
                "root must be a non-root directory".into(),
            ));
        }
        validate_rule_metadata(&rules)?;
        let mut entries = Vec::new();
        for rule in rules {
            let mut paths = Vec::new();
            for pattern in &rule.paths {
                // Absolute compatibility paths are relative to the explicitly
                // supplied spelling, not normalized with Path::components.
                let path = parse_path_pattern(pattern, supplied).map_err(|reason| {
                    RuleError::UnsafePath {
                        rule: rule.id.clone(),
                        path: pattern.clone(),
                        reason,
                    }
                })?;
                paths.push(path);
            }
            entries.push(ValidatedRule { rule, paths });
        }
        Ok(Self {
            root: Some(root),
            entries,
        })
    }

    pub(crate) fn empty() -> Self {
        Self {
            root: None,
            entries: Vec::new(),
        }
    }
}

/// Parse and validate the rules compiled into the binary.
pub fn builtin_rules() -> Result<Vec<Rule>, RuleError> {
    let mut rules = Vec::with_capacity(BUILTIN_RULE_SOURCES.len());
    for (name, source) in BUILTIN_RULE_SOURCES {
        rules.push(parse_rule(name, source)?);
    }
    validate_rules(&rules)?;
    Ok(rules)
}

/// Load every `*.toml` rule under `dir`, recursively, then validate them.
pub fn load_rules_from_dir(dir: impl AsRef<Path>) -> Result<Vec<Rule>, RuleError> {
    let dir = dir.as_ref();
    let mut files: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|source| RuleError::ReadDir {
            path: dir.display().to_string(),
            source,
        })?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "toml")
        {
            files.push(entry.into_path());
        }
    }

    files.sort();
    let mut rules = Vec::with_capacity(files.len());
    for file in files {
        let source = fs::read_to_string(&file).map_err(|source| RuleError::ReadFile {
            path: file.display().to_string(),
            source,
        })?;
        rules.push(parse_rule(&file.display().to_string(), &source)?);
    }

    validate_rules(&rules)?;
    Ok(rules)
}

fn parse_rule(name: &str, source: &str) -> Result<Rule, RuleError> {
    toml::from_str(source).map_err(|source| RuleError::Parse {
        name: name.to_string(),
        source,
    })
}

fn validate_rules(rules: &[Rule]) -> Result<(), RuleError> {
    validate_rule_metadata(rules)?;
    for rule in rules {
        for path in &rule.paths {
            validate_path_pattern(path).map_err(|reason| RuleError::UnsafePath {
                rule: rule.id.clone(),
                path: path.clone(),
                reason,
            })?;
        }
    }
    Ok(())
}

fn validate_rule_metadata(rules: &[Rule]) -> Result<(), RuleError> {
    let mut ids = HashSet::new();

    for rule in rules {
        if rule.id.is_empty()
            || rule.id.len() > 128
            || !rule
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || !rule.id.as_bytes()[0].is_ascii_alphanumeric()
        {
            return Err(RuleError::InvalidId(rule.id.clone()));
        }
        if !ids.insert(rule.id.clone()) {
            return Err(RuleError::DuplicateId(rule.id.clone()));
        }
        if rule.paths.is_empty() {
            return Err(RuleError::NoPaths {
                rule: rule.id.clone(),
            });
        }
        let mut seen_os = HashSet::new();
        if rule.os.is_empty() {
            return Err(RuleError::InvalidOs {
                rule: rule.id.clone(),
                os: "(empty)".into(),
            });
        }
        for os in &rule.os {
            if !matches!(os.as_str(), "macos" | "windows" | "linux") || !seen_os.insert(os) {
                return Err(RuleError::InvalidOs {
                    rule: rule.id.clone(),
                    os: os.clone(),
                });
            }
        }
    }

    Ok(())
}

/// Validate a rule path before it is allowed to participate in a cleanup.
///
/// The v0.1 contract is deliberately narrow: every rule path must be inside
/// the user's home directory (either `~/...` or an absolute path under it)
/// and specific. Broad roots such as `~/Library/*` and any `..` traversal are
/// rejected. This is a safety invariant, not a convenience check.
pub fn validate_path_pattern(pattern: &str) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "home directory is unknown".to_string())?;
    parse_path_pattern(pattern, &home).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rules_are_valid() {
        let rules = builtin_rules().expect("built-in rules should parse");
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|rule| rule.id == "xcode-derived-data"));
        assert!(rules.iter().any(|rule| rule.id == "npm-cache"));
    }

    #[test]
    fn rejects_home_and_broad_paths() {
        for pattern in [
            "~",
            "~/",
            "~/Library/*",
            "~/Downloads/*",
            "~/Documents/*",
            "~/Library/../Secrets/*",
            "/Library/Caches/*",
            "C:/Users/me/AppData/*",
        ] {
            assert!(
                validate_path_pattern(pattern).is_err(),
                "{pattern} should be rejected"
            );
        }
    }

    #[test]
    fn accepts_specific_home_relative_paths() {
        for pattern in [
            "~/Library/Caches/Homebrew/*",
            "~/Library/Developer/Xcode/DerivedData/*",
            "~/.npm/_cacache/*",
            "~/.Trash/*",
        ] {
            assert!(
                validate_path_pattern(pattern).is_ok(),
                "{pattern} should be accepted"
            );
        }
    }

    #[test]
    fn accepts_absolute_paths_under_home() {
        if let Some(home) = dirs::home_dir() {
            let path = home.join("prune-fixture").join("cache");
            assert!(validate_path_pattern(&path.to_string_lossy().replace('\\', "/")).is_ok());
        }
    }

    #[test]
    fn rejects_native_backslash_paths() {
        assert!(validate_path_pattern(r"~\app\cache").is_err());
        if let Some(home) = dirs::home_dir() {
            let path = home.join("prune-fixture").join("cache");
            let native_backslashes = path.to_string_lossy().replace('/', "\\");
            assert!(validate_path_pattern(&native_backslashes).is_err());
        }
    }

    #[test]
    fn rejects_all_confirmed_path_bypasses() {
        for pattern in [
            "~//absolute/cache",
            "~/app//cache",
            "~/app/cache/",
            "~/app/../cache",
            "~/app/./cache",
            "~/.[.]/outside/*",
            "~/app\\..\\outside/cache",
            "~/app/\\absolute",
            " ~/app/cache",
            "~/app/cache ",
            "~/app/\tcache",
            "~/app/\ncache",
            "~/app/\0cache",
            "~/app/**",
            "~/app/c?che",
            "~/app/[cache]",
            "~/app/{a,b}",
            "~/app/*/cache",
            "~/app/cache*",
            "~/app/C:/cache",
            "~/app/.. /cache",
            "~/app/cache.",
            "~/app/CON.txt",
            "~/app/COM1",
            "~/library/*",
            "~/DOWNLOADS/*",
            "~/app/$HOME",
            "~/app/~outside",
            "//server/share/cache",
            "\\\\server\\share\\cache",
        ] {
            assert!(validate_path_pattern(pattern).is_err(), "{pattern:?}");
        }
    }

    fn fixture_rule() -> Rule {
        builtin_rules().unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn public_capability_validates_programmatically_constructed_rules() {
        let root = tempfile::tempdir().unwrap();
        for path in ["~//absolute/cache", "~/.[.]/outside/*", "~/app\\../cache"] {
            let mut rule = fixture_rule();
            rule.paths = vec![path.into()];
            assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err());
        }
        let mut rule = fixture_rule();
        rule.paths = vec!["~/app/cache".into()];
        let validated =
            ValidatedRuleSet::for_trusted_root(vec![rule.clone()], root.path()).unwrap();
        rule.paths[0] = "~//outside/cache".into();
        assert_eq!(validated.entries[0].paths[0].components, ["app", "cache"]);
        assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err());
    }

    #[test]
    fn rejects_bad_ids_empty_duplicate_and_unknown_os() {
        let root = tempfile::tempdir().unwrap();
        for id in ["", "with space", "../id", "UPPER", "-prefix", "id\n"] {
            let mut rule = fixture_rule();
            rule.id = id.into();
            assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err());
        }
        for os in [vec![], vec!["linux", "linux"], vec!["other"], vec!["Linux"]] {
            let mut rule = fixture_rule();
            rule.os = os.into_iter().map(str::to_owned).collect();
            assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err());
        }
        let rule = fixture_rule();
        assert!(ValidatedRuleSet::for_trusted_root(vec![rule.clone(), rule], root.path()).is_err());
    }

    #[test]
    fn schema_rejects_unknown_fields() {
        let source = BUILTIN_RULE_SOURCES[0].1;
        assert!(parse_rule("unknown", &format!("{source}\nunexpected = true\n")).is_err());
    }

    #[test]
    fn trusted_root_is_explicit_and_never_filesystem_root() {
        let root = tempfile::tempdir().unwrap();
        let filesystem_root = root.path().ancestors().last().unwrap();
        assert!(ValidatedRuleSet::for_trusted_root(vec![], filesystem_root).is_err());
        assert!(ValidatedRuleSet::for_trusted_root(vec![], Path::new(".")).is_err());
        let mut rule = fixture_rule();
        rule.paths = vec!["/outside/app/cache".into()];
        assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err());
        let mut rule = fixture_rule();
        rule.paths = vec!["~/app/cache".into()];
        assert!(ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_ok());
    }

    #[test]
    fn absolute_patterns_are_not_normalized_before_validation() {
        let root = tempfile::tempdir().unwrap();
        let prefix = root.path().to_str().unwrap().replace('\\', "/");
        for suffix in [
            "/app/../outside",
            "//outside/cache",
            "/app/./cache",
            "/app//cache",
            "/app/cache/",
            "/.[.]/outside/*",
        ] {
            let mut rule = fixture_rule();
            rule.paths = vec![format!("{prefix}{suffix}")];
            assert!(
                ValidatedRuleSet::for_trusted_root(vec![rule], root.path()).is_err(),
                "{suffix}"
            );
        }
    }
}
