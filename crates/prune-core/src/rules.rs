use crate::model::Rule;
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
    let mut ids = HashSet::new();

    for rule in rules {
        if !ids.insert(rule.id.clone()) {
            return Err(RuleError::DuplicateId(rule.id.clone()));
        }
        if rule.paths.is_empty() {
            return Err(RuleError::NoPaths {
                rule: rule.id.clone(),
            });
        }
        for os in &rule.os {
            if !matches!(os.as_str(), "macos" | "windows" | "linux") {
                return Err(RuleError::InvalidOs {
                    rule: rule.id.clone(),
                    os: os.clone(),
                });
            }
        }
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

/// Validate a rule path before it is allowed to participate in a cleanup.
///
/// The v0.1 contract is deliberately narrow: every rule path must be inside
/// the user's home directory (either `~/...` or an absolute path under it)
/// and specific. Broad roots such as `~/Library/*` and any `..` traversal are
/// rejected. This is a safety invariant, not a convenience check.
pub fn validate_path_pattern(pattern: &str) -> Result<(), String> {
    const BROAD_ROOTS: &[&str] = &[
        "Library",
        "Documents",
        "Desktop",
        "Downloads",
        "Pictures",
        "Movies",
        "Music",
        "Applications",
        "Projects",
        "Developer",
        "Public",
    ];

    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("path is empty".to_string());
    }
    if pattern.contains('\0') {
        return Err("path contains a NUL byte".to_string());
    }
    if pattern.contains('$') {
        return Err("environment variables are not supported yet".to_string());
    }
    let rest: String = if let Some(rest) = pattern.strip_prefix("~/") {
        rest.to_string()
    } else if Path::new(pattern).is_absolute() {
        let home = dirs::home_dir().ok_or_else(|| "home directory is unknown".to_string())?;
        let rest = Path::new(pattern)
            .strip_prefix(&home)
            .map_err(|_| "absolute paths must be under the home directory".to_string())?;
        rest.to_string_lossy().replace('\\', "/")
    } else {
        return Err("path must start with ~/ or be under the home directory".to_string());
    };

    if rest.is_empty() {
        return Err("path must not be the home directory".to_string());
    }

    let components: Vec<&str> = rest
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if components.is_empty() {
        return Err("path must not be the home directory".to_string());
    }
    if components
        .iter()
        .any(|component| *component == ".." || *component == ".")
    {
        return Err("path must not contain . or .. components".to_string());
    }
    if components.len() < 2 {
        return Err("path is too broad; use a more specific path".to_string());
    }
    if matches!(components[0], "*" | "**") {
        return Err("path is too broad; use a more specific path".to_string());
    }
    if BROAD_ROOTS.contains(&components[0])
        && components.len() <= 2
        && matches!(components[1], "*" | "**")
    {
        return Err("path is too broad; use a more specific path".to_string());
    }

    Ok(())
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
            assert!(validate_path_pattern(&path.to_string_lossy()).is_ok());
        }
    }
}
