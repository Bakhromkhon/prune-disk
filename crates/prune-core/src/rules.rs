use crate::model::{Category, Recreate, Risk, Rule, ScanRootKind, ScanScope};
use crate::path_policy::{ValidatedPath, parse_path_pattern};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

const RULE_SET_DIGEST_PREFIX: &str = "sha256:";

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
    root_kind: ScanRootKind,
    pub(crate) entries: Vec<ValidatedRule>,
}

#[derive(Serialize)]
struct CanonicalRule<'a> {
    id: &'a str,
    name: &'a str,
    description: &'a str,
    os: Vec<&'a str>,
    category: Category,
    risk: Risk,
    recreate: Recreate,
    paths: Vec<String>,
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
        Self::for_root(rules, home, ScanRootKind::Home)
    }

    /// Explicitly trust a directory as the scan boundary (e.g. an isolated test
    /// fixture). The filesystem/drive root is never accepted. The caller must
    /// own this trust decision; do not derive it from untrusted rule content.
    pub fn for_trusted_root(rules: Vec<Rule>, root: impl AsRef<Path>) -> Result<Self, RuleError> {
        Self::for_root(rules, root, ScanRootKind::Custom)
    }

    fn for_root(
        rules: Vec<Rule>,
        root: impl AsRef<Path>,
        root_kind: ScanRootKind,
    ) -> Result<Self, RuleError> {
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
            root_kind,
            entries,
        })
    }

    /// Stable SHA-256 fingerprint of the validated rules relevant to `os`,
    /// encoded as `sha256:<64 lowercase hex characters>`. The hashed preimage
    /// includes the `prune-rule-set-v1` domain separator.
    ///
    /// Rules are serialized as compact JSON after sorting rules by id and each
    /// rule's OS names and normalized path patterns. Root paths are excluded.
    /// Every rule for `os` is included before category/risk filters and overlap
    /// resolution. The digest is for comparison, not a security signature.
    pub fn rule_set_digest_for_os(&self, os: &str) -> String {
        canonical_rule_set_digest(&self.entries, os)
    }

    pub(crate) fn scan_scope(
        &self,
        os: &str,
        categories: &[Category],
        max_risk: Option<Risk>,
        min_size_bytes: u64,
    ) -> ScanScope {
        let mut categories = categories.to_vec();
        categories.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        categories.dedup();

        ScanScope {
            root_kind: self.root_kind,
            comparable: false,
            categories,
            max_risk,
            min_size_bytes,
            rule_set_digest: self.rule_set_digest_for_os(os),
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            root: None,
            root_kind: ScanRootKind::Empty,
            entries: Vec::new(),
        }
    }
}

fn canonical_rule_set_digest(entries: &[ValidatedRule], os: &str) -> String {
    let mut sorted_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.rule.applies_to(os))
        .collect();
    sorted_entries.sort_by(|left, right| left.rule.id.cmp(&right.rule.id));

    let canonical_rules: Vec<_> = sorted_entries
        .into_iter()
        .map(|entry| {
            let mut os: Vec<_> = entry.rule.os.iter().map(String::as_str).collect();
            os.sort_unstable();
            let mut paths: Vec<_> = entry.paths.iter().map(canonical_path).collect();
            paths.sort_unstable();

            CanonicalRule {
                id: &entry.rule.id,
                name: &entry.rule.name,
                description: &entry.rule.description,
                os,
                category: entry.rule.category,
                risk: entry.rule.risk,
                recreate: entry.rule.recreate,
                paths,
            }
        })
        .collect();

    // CanonicalRule has a fixed field order and only contains strings, vectors,
    // and enums, so serialization is deterministic and cannot fail.
    let bytes = serde_json::to_vec(&canonical_rules).expect("canonical validated rules serialize");
    let mut hasher = Sha256::new();
    hasher.update(b"prune-rule-set-v1\0");
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hexadecimal = String::with_capacity(RULE_SET_DIGEST_PREFIX.len() + digest.len() * 2);
    hexadecimal.push_str(RULE_SET_DIGEST_PREFIX);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        hexadecimal.push(HEX[(byte >> 4) as usize] as char);
        hexadecimal.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hexadecimal
}

fn canonical_path(path: &ValidatedPath) -> String {
    let mut canonical = path.components.join("/");
    if path.children {
        canonical.push_str("/*");
    }
    canonical
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
    fn validated_root_kind_and_comparability_are_explicit() {
        let empty = ValidatedRuleSet::empty();
        assert_eq!(empty.root_kind, ScanRootKind::Empty);
        assert!(!empty.scan_scope("linux", &[], None, 0).comparable);

        let custom_root = tempfile::tempdir().unwrap();
        let custom = ValidatedRuleSet::for_trusted_root(vec![], custom_root.path()).unwrap();
        assert_eq!(custom.root_kind, ScanRootKind::Custom);
        assert!(!custom.scan_scope("linux", &[], None, 0).comparable);

        if dirs::home_dir().is_some() {
            let home = ValidatedRuleSet::new(builtin_rules().unwrap()).unwrap();
            assert_eq!(home.root_kind, ScanRootKind::Home);
            assert!(!home.scan_scope("macos", &[], None, 0).comparable);
        }
    }

    #[test]
    fn rule_set_digest_is_canonical_for_rule_os_and_path_order() {
        let root = tempfile::tempdir().unwrap();
        let mut alpha = fixture_rule();
        alpha.id = "alpha".into();
        alpha.os = vec!["macos".into(), "linux".into()];
        alpha.paths = vec!["~/app/z-cache".into(), "~/app/a-cache".into()];

        let mut beta = fixture_rule();
        beta.id = "beta".into();
        beta.os = vec!["linux".into()];
        beta.paths = vec!["~/build/cache".into()];

        let original =
            ValidatedRuleSet::for_trusted_root(vec![alpha.clone(), beta.clone()], root.path())
                .unwrap();

        alpha.os.reverse();
        alpha.paths.reverse();
        let reordered =
            ValidatedRuleSet::for_trusted_root(vec![beta.clone(), alpha.clone()], root.path())
                .unwrap();

        assert_eq!(
            original.rule_set_digest_for_os("linux"),
            reordered.rule_set_digest_for_os("linux")
        );
        assert!(
            original
                .rule_set_digest_for_os("linux")
                .starts_with("sha256:")
        );
        assert_eq!(original.rule_set_digest_for_os("linux").len(), 71);
        assert_ne!(
            original.rule_set_digest_for_os("linux"),
            original.rule_set_digest_for_os("windows")
        );

        alpha.description.push_str(" changed");
        let changed = ValidatedRuleSet::for_trusted_root(vec![alpha, beta], root.path()).unwrap();
        assert_ne!(
            original.rule_set_digest_for_os("linux"),
            changed.rule_set_digest_for_os("linux")
        );
    }

    #[test]
    fn absolute_rule_paths_are_normalized_without_hashing_the_trusted_root() {
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let make_rule = |root: &Path| {
            let mut rule = fixture_rule();
            rule.os = vec!["linux".into()];
            rule.paths = vec![root.join("app/cache").to_string_lossy().replace('\\', "/")];
            rule
        };

        let first = ValidatedRuleSet::for_trusted_root(
            vec![make_rule(first_root.path())],
            first_root.path(),
        )
        .unwrap();
        let second = ValidatedRuleSet::for_trusted_root(
            vec![make_rule(second_root.path())],
            second_root.path(),
        )
        .unwrap();

        assert_eq!(
            first.rule_set_digest_for_os("linux"),
            second.rule_set_digest_for_os("linux")
        );
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
