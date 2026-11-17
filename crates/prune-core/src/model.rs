use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// How risky it is to remove a finding.
///
/// The ordering is intentional: `Safe < Low < Medium < High < Never`.
/// A planner can therefore use `risk <= max_risk` as a filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Low,
    Medium,
    High,
    Never,
}

impl Risk {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Never => "never",
        }
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A broad grouping used by the UI and the future planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Developer,
    Browser,
    System,
    Trash,
    Apps,
    Media,
    Other,
}

impl Category {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Developer => "developer",
            Self::Browser => "browser",
            Self::System => "system",
            Self::Trash => "trash",
            Self::Apps => "apps",
            Self::Media => "media",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the data can be recreated if it is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Recreate {
    Automatic,
    Download,
    Rebuild,
    Manual,
    Impossible,
    Unknown,
}

impl Recreate {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Download => "download",
            Self::Rebuild => "rebuild",
            Self::Manual => "manual",
            Self::Impossible => "impossible",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Recreate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A declarative rule describing a known cache or disposable data location.
///
/// Rules are data, not code: they cannot run commands or elevate privileges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub os: Vec<String>,
    pub category: Category,
    pub risk: Risk,
    pub recreate: Recreate,
    pub paths: Vec<String>,
}

impl Rule {
    /// Returns true when this rule is relevant for `os` (for example `"macos"`).
    pub fn applies_to(&self, os: &str) -> bool {
        self.os.iter().any(|candidate| candidate == os)
    }
}

/// One concrete path matched by a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub rule_name: String,
    pub category: Category,
    pub risk: Risk,
    pub recreate: Recreate,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_count: u64,
    pub last_modified: Option<u64>,
    pub description: String,
}

/// A non-fatal problem encountered during a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

/// The result of a read-only scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub schema_version: u32,
    pub platform: String,
    pub scanned_at: u64,
    pub total_bytes: u64,
    pub total_files: u64,
    pub findings: Vec<Finding>,
    pub errors: Vec<ScanError>,
}

impl ScanReport {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}
