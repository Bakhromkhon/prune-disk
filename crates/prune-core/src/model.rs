use serde::de::{self, Deserializer, Visitor};
use serde::ser::Error as _;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;
use std::path::PathBuf;

const JS_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;
const RULE_SET_DIGEST_PREFIX: &str = "sha256:";

mod js_safe_u64 {
    use super::{JS_SAFE_INTEGER_MAX, Visitor};
    use serde::{Deserializer, Serializer, de::Error};
    use std::fmt;

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if *value <= JS_SAFE_INTEGER_MAX {
            serializer.serialize_u64(*value)
        } else {
            serializer.serialize_str(&value.to_string())
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(U64Visitor)
    }

    struct U64Visitor;

    impl<'de> Visitor<'de> for U64Visitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an unsigned integer or its decimal string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            u64::try_from(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            value.parse().map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: Error,
        {
            self.visit_str(&value)
        }
    }
}

mod js_safe_option_u64 {
    use super::JS_SAFE_INTEGER_MAX;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) if *value <= JS_SAFE_INTEGER_MAX => serializer.serialize_u64(*value),
            Some(value) => serializer.serialize_str(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum NumberOrString {
            Number(u64),
            String(String),
        }

        Option::<NumberOrString>::deserialize(deserializer)?
            .map(|value| match value {
                NumberOrString::Number(value) => Ok(value),
                NumberOrString::String(value) => value.parse().map_err(D::Error::custom),
            })
            .transpose()
    }
}

mod js_safe_timestamp {
    use super::JS_SAFE_INTEGER_MAX;
    use serde::de::Error;
    use serde::ser::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if *value > JS_SAFE_INTEGER_MAX {
            return Err(S::Error::custom(
                "scanned_at exceeds JavaScript's maximum safe integer",
            ));
        }
        serializer.serialize_u64(*value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        if value > JS_SAFE_INTEGER_MAX {
            return Err(D::Error::custom(
                "scanned_at exceeds JavaScript's maximum safe integer",
            ));
        }
        Ok(value)
    }
}

mod display_path {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::{Path, PathBuf};

    pub fn serialize<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(path.to_string_lossy().as_ref())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(PathBuf::from)
    }
}

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
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub rule_name: String,
    pub category: Category,
    pub risk: Risk,
    pub recreate: Recreate,
    /// Native path in memory; serialized paths are display-only. Never treat an
    /// imported report path as authority for filesystem actions.
    #[serde(with = "display_path")]
    pub path: PathBuf,
    /// Omitted for ordinary UTF-8 paths. `lossy_utf8` warns that `path` is only
    /// a display representation and cannot round-trip to the original bytes.
    #[serde(default)]
    pub path_encoding: PathEncoding,
    #[serde(with = "js_safe_u64")]
    pub size_bytes: u64,
    #[serde(with = "js_safe_u64")]
    pub file_count: u64,
    #[serde(default, with = "js_safe_option_u64")]
    pub last_modified: Option<u64>,
    pub description: String,
    #[serde(default)]
    pub byte_accounting: ByteAccounting,
    #[serde(default)]
    pub partial: bool,
}

/// Encoding fidelity of a finding's display path in JSON.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathEncoding {
    #[default]
    Utf8,
    LossyUtf8,
}

impl PathEncoding {
    const fn is_utf8(&self) -> bool {
        matches!(self, Self::Utf8)
    }
}

#[derive(Serialize)]
struct FindingForSerialization<'a> {
    rule_id: &'a str,
    rule_name: &'a str,
    category: Category,
    risk: Risk,
    recreate: Recreate,
    #[serde(with = "display_path")]
    path: PathBuf,
    #[serde(skip_serializing_if = "PathEncoding::is_utf8")]
    path_encoding: PathEncoding,
    #[serde(with = "js_safe_u64")]
    size_bytes: u64,
    #[serde(with = "js_safe_u64")]
    file_count: u64,
    #[serde(with = "js_safe_option_u64")]
    last_modified: Option<u64>,
    description: &'a str,
    byte_accounting: ByteAccounting,
    partial: bool,
}

impl Serialize for Finding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let path_encoding = if self.path.to_str().is_none() {
            PathEncoding::LossyUtf8
        } else {
            self.path_encoding
        };
        FindingForSerialization {
            rule_id: &self.rule_id,
            rule_name: &self.rule_name,
            category: self.category,
            risk: self.risk,
            recreate: self.recreate,
            path: self.path.clone(),
            path_encoding,
            size_bytes: self.size_bytes,
            file_count: self.file_count,
            last_modified: self.last_modified,
            description: &self.description,
            byte_accounting: self.byte_accounting,
            partial: self.partial,
        }
        .serialize(serializer)
    }
}

/// Sizes are logical file lengths, not allocated or reclaimable disk space.
/// Hard-linked files may be counted more than once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteAccounting {
    #[default]
    Logical,
}

/// Whether the scan completed with complete or incomplete coverage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanCompleteness {
    Complete,
    Partial,
    Cancelled,
    /// Used when reading reports that predate the completeness field.
    #[default]
    Unknown,
}

/// Privacy-safe classification of the scan boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRootKind {
    Home,
    Custom,
    /// No trusted root was configured.
    #[default]
    Empty,
}

/// A privacy-safe description of filters and rules used by a scan.
///
/// `root_kind` identifies only the boundary class, never its actual path. An
/// empty `categories` list means all categories. `min_size_bytes` is the
/// configured lower bound for a finding and is serialized as a decimal string
/// when it exceeds JavaScript's maximum safe integer. Reported byte totals are
/// logical lengths and do not claim exact reclaimable disk space. The
/// `comparable` compatibility flag is always false. This description does not
/// prove two reports share the same private root identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanScope {
    pub root_kind: ScanRootKind,
    /// Compatibility flag; public report scopes must keep this false until a
    /// private local root identity can prove pairwise comparability.
    pub comparable: bool,
    pub categories: Vec<Category>,
    pub max_risk: Option<Risk>,
    pub min_size_bytes: u64,
    /// SHA-256 of canonical validated rule data (`sha256:<64 lowercase hex
    /// characters>`). The preimage is domain-separated with
    /// `prune-rule-set-v1`. This is a comparison fingerprint, not a signature.
    pub rule_set_digest: String,
}

#[derive(Serialize)]
struct ScanScopeForSerialization<'a> {
    root_kind: ScanRootKind,
    comparable: bool,
    categories: &'a [Category],
    max_risk: Option<Risk>,
    #[serde(with = "js_safe_u64")]
    min_size_bytes: u64,
    rule_set_digest: &'a str,
}

#[derive(Deserialize)]
struct ScanScopeWire {
    root_kind: ScanRootKind,
    comparable: bool,
    categories: Vec<Category>,
    #[serde(deserialize_with = "deserialize_required_option")]
    max_risk: Option<Risk>,
    #[serde(with = "js_safe_u64")]
    min_size_bytes: u64,
    rule_set_digest: String,
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl Serialize for ScanScope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Err(message) = validate_v2_scope(self) {
            return Err(S::Error::custom(message));
        }
        ScanScopeForSerialization {
            root_kind: self.root_kind,
            comparable: self.comparable,
            categories: &self.categories,
            max_risk: self.max_risk,
            min_size_bytes: self.min_size_bytes,
            rule_set_digest: &self.rule_set_digest,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ScanScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScanScopeWire::deserialize(deserializer)?;
        let scope = Self {
            root_kind: wire.root_kind,
            comparable: wire.comparable,
            categories: wire.categories,
            max_risk: wire.max_risk,
            min_size_bytes: wire.min_size_bytes,
            rule_set_digest: wire.rule_set_digest,
        };
        validate_v2_scope(&scope).map_err(de::Error::custom)?;
        Ok(scope)
    }
}

/// A non-fatal problem encountered during a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

/// The result of a read-only scan.
///
/// JSON encodes `u64` values as numbers up to JavaScript's maximum safe integer
/// and as decimal strings above that limit.
#[derive(Debug, Clone)]
pub struct ScanReport {
    pub schema_version: u32,
    pub platform: String,
    pub scanned_at: u64,
    pub total_bytes: u64,
    pub total_files: u64,
    pub findings: Vec<Finding>,
    pub errors: Vec<ScanError>,
    /// `Unknown` is intentional for older reports without this field.
    pub completeness: ScanCompleteness,
    /// Absent from v1 reports; v2 reports always include a scope.
    pub scope: Option<ScanScope>,
    pub byte_accounting: ByteAccounting,
    /// Compatibility alias that must match `completeness`.
    pub partial: bool,
    /// Compatibility alias that must match `completeness`.
    pub cancelled: bool,
    /// Errors omitted after the bounded diagnostic buffer filled.
    pub suppressed_errors: u64,
}

#[derive(Serialize)]
struct ScanReportForSerialization<'a> {
    schema_version: u32,
    platform: &'a str,
    #[serde(with = "js_safe_timestamp")]
    scanned_at: u64,
    #[serde(with = "js_safe_u64")]
    total_bytes: u64,
    #[serde(with = "js_safe_u64")]
    total_files: u64,
    findings: &'a [Finding],
    errors: &'a [ScanError],
    completeness: ScanCompleteness,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a ScanScope>,
    byte_accounting: ByteAccounting,
    partial: bool,
    cancelled: bool,
    #[serde(with = "js_safe_u64")]
    suppressed_errors: u64,
}

impl Serialize for ScanReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if !matches!(self.schema_version, 1 | 2) {
            return Err(S::Error::custom(format!(
                "unsupported schema_version {}; supported versions are 1 and 2",
                self.schema_version
            )));
        }
        if self.schema_version == 1 && self.completeness == ScanCompleteness::Unknown {
            return Err(S::Error::custom(
                "schema_version 1 cannot faithfully encode unknown completeness",
            ));
        }
        if self.schema_version == 1 && !v1_values_fit_js_safe_integers(self) {
            return Err(S::Error::custom(
                "schema_version 1 requires JavaScript-safe integer values",
            ));
        }
        let expected_partial = matches!(
            self.completeness,
            ScanCompleteness::Partial | ScanCompleteness::Cancelled
        );
        let expected_cancelled = self.completeness == ScanCompleteness::Cancelled;
        if self.partial != expected_partial || self.cancelled != expected_cancelled {
            return Err(S::Error::custom(
                "legacy partial/cancelled flags contradict completeness",
            ));
        }
        let has_partial_evidence = !self.errors.is_empty()
            || self.suppressed_errors > 0
            || self.findings.iter().any(|finding| finding.partial);
        if self.schema_version == 2 {
            let scope = self
                .scope
                .as_ref()
                .ok_or_else(|| S::Error::custom("v2 reports must include a scope"))?;
            if let Err(message) = validate_v2_scope(scope) {
                return Err(S::Error::custom(message));
            }
            if has_partial_evidence
                && matches!(
                    self.completeness,
                    ScanCompleteness::Complete | ScanCompleteness::Unknown
                )
            {
                return Err(S::Error::custom(
                    "completeness contradicts partial scan evidence",
                ));
            }
        }

        ScanReportForSerialization {
            schema_version: self.schema_version,
            platform: &self.platform,
            scanned_at: self.scanned_at,
            total_bytes: self.total_bytes,
            total_files: self.total_files,
            findings: &self.findings,
            errors: &self.errors,
            completeness: self.completeness,
            scope: self.scope.as_ref(),
            byte_accounting: self.byte_accounting,
            partial: self.partial,
            cancelled: self.cancelled,
            suppressed_errors: self.suppressed_errors,
        }
        .serialize(serializer)
    }
}

fn v1_values_fit_js_safe_integers(report: &ScanReport) -> bool {
    report.scanned_at <= JS_SAFE_INTEGER_MAX
        && report.total_bytes <= JS_SAFE_INTEGER_MAX
        && report.total_files <= JS_SAFE_INTEGER_MAX
        && report.suppressed_errors <= JS_SAFE_INTEGER_MAX
        && report
            .scope
            .as_ref()
            .is_none_or(|scope| scope.min_size_bytes <= JS_SAFE_INTEGER_MAX)
        && report.findings.iter().all(|finding| {
            finding.size_bytes <= JS_SAFE_INTEGER_MAX
                && finding.file_count <= JS_SAFE_INTEGER_MAX
                && finding
                    .last_modified
                    .is_none_or(|time| time <= JS_SAFE_INTEGER_MAX)
        })
}

impl ScanReport {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Public reports do not carry a private root identity, so they cannot
    /// establish pairwise comparability even when their visible scopes match.
    pub fn is_comparable_with(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Deserialize)]
struct ScanReportWire {
    schema_version: u32,
    platform: String,
    #[serde(with = "js_safe_timestamp")]
    scanned_at: u64,
    #[serde(with = "js_safe_u64")]
    total_bytes: u64,
    #[serde(with = "js_safe_u64")]
    total_files: u64,
    findings: Vec<Finding>,
    errors: Vec<ScanError>,
    #[serde(default)]
    completeness: Option<ScanCompleteness>,
    #[serde(default)]
    scope: Option<ScanScope>,
    #[serde(default)]
    byte_accounting: ByteAccounting,
    #[serde(default)]
    partial: Option<bool>,
    #[serde(default)]
    cancelled: Option<bool>,
    #[serde(default, with = "js_safe_u64")]
    suppressed_errors: u64,
}

impl<'de> Deserialize<'de> for ScanReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScanReportWire::deserialize(deserializer)?;
        let has_partial_evidence = !wire.errors.is_empty()
            || wire.suppressed_errors > 0
            || wire.findings.iter().any(|finding| finding.partial);

        let (completeness, scope) = match wire.schema_version {
            2 => {
                let completeness = wire
                    .completeness
                    .ok_or_else(|| de::Error::missing_field("completeness"))?;
                let scope = wire
                    .scope
                    .ok_or_else(|| de::Error::missing_field("scope"))?;
                validate_v2_scope(&scope).map_err(de::Error::custom)?;
                let expected_partial = matches!(
                    completeness,
                    ScanCompleteness::Partial | ScanCompleteness::Cancelled
                );
                let expected_cancelled = completeness == ScanCompleteness::Cancelled;

                if wire.partial.is_some_and(|value| value != expected_partial)
                    || wire
                        .cancelled
                        .is_some_and(|value| value != expected_cancelled)
                {
                    return Err(de::Error::custom(
                        "legacy partial/cancelled flags contradict completeness",
                    ));
                }
                if has_partial_evidence
                    && matches!(
                        completeness,
                        ScanCompleteness::Complete | ScanCompleteness::Unknown
                    )
                {
                    return Err(de::Error::custom(
                        "completeness contradicts partial scan evidence",
                    ));
                }
                (completeness, Some(scope))
            }
            1 => {
                let cancelled = wire.cancelled == Some(true)
                    || wire.completeness == Some(ScanCompleteness::Cancelled);
                let completeness = if cancelled {
                    ScanCompleteness::Cancelled
                } else if has_partial_evidence || wire.partial == Some(true) {
                    ScanCompleteness::Partial
                } else if let Some(completeness) = wire.completeness {
                    completeness
                } else if wire.partial == Some(false) {
                    ScanCompleteness::Complete
                } else {
                    ScanCompleteness::Unknown
                };
                (completeness, wire.scope)
            }
            version => {
                return Err(de::Error::custom(format!(
                    "unsupported schema_version {version}; supported versions are 1 and 2"
                )));
            }
        };

        let cancelled = completeness == ScanCompleteness::Cancelled;
        let partial = matches!(
            completeness,
            ScanCompleteness::Partial | ScanCompleteness::Cancelled
        );

        Ok(Self {
            schema_version: wire.schema_version,
            platform: wire.platform,
            scanned_at: wire.scanned_at,
            total_bytes: wire.total_bytes,
            total_files: wire.total_files,
            findings: wire.findings,
            errors: wire.errors,
            completeness,
            scope,
            byte_accounting: wire.byte_accounting,
            partial,
            cancelled,
            suppressed_errors: wire.suppressed_errors,
        })
    }
}

fn validate_v2_scope(scope: &ScanScope) -> Result<(), &'static str> {
    if scope.comparable {
        return Err("scope.comparable must be false without a private root identity");
    }
    let Some(digest_hex) = scope.rule_set_digest.strip_prefix(RULE_SET_DIGEST_PREFIX) else {
        return Err("scope rule_set_digest must use the sha256 prefix");
    };
    if digest_hex.len() != 64
        || !digest_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("scope rule_set_digest must have 64 lowercase SHA-256 hex characters");
    }
    let mut canonical_categories = scope.categories.clone();
    canonical_categories.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    canonical_categories.dedup();
    if canonical_categories != scope.categories {
        return Err("scope categories must be sorted and unique");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_RULE_SET_DIGEST: &str =
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn older_reports_deserialize_with_unknown_completeness() {
        let report: ScanReport = toml::from_str(
            r#"
schema_version = 1
platform = "linux"
scanned_at = 0
total_bytes = 0
total_files = 0
findings = []
errors = []
"#,
        )
        .unwrap();
        assert_eq!(report.completeness, ScanCompleteness::Unknown);
        assert_eq!(report.scope, None);
        assert_eq!(report.byte_accounting, ByteAccounting::Logical);
        assert!(!report.partial);
        assert!(!report.cancelled);
        assert_eq!(report.suppressed_errors, 0);
    }

    #[test]
    fn v1_completeness_deserialization_preserves_partial_presence_and_evidence() {
        let v1 = || {
            json!({
                "schema_version": 1,
                "platform": "linux",
                "scanned_at": 0,
                "total_bytes": 0,
                "total_files": 0,
                "findings": [],
                "errors": []
            })
        };

        let unknown: ScanReport = serde_json::from_value(v1()).unwrap();
        assert_eq!(unknown.completeness, ScanCompleteness::Unknown);

        let mut explicitly_complete = v1();
        explicitly_complete["partial"] = json!(false);
        let complete: ScanReport = serde_json::from_value(explicitly_complete).unwrap();
        assert_eq!(complete.completeness, ScanCompleteness::Complete);
        assert!(!complete.partial && !complete.cancelled);

        let mut errors = v1();
        errors["partial"] = json!(false);
        errors["errors"] = json!([{"path": "/cache", "message": "unreadable"}]);
        let partial: ScanReport = serde_json::from_value(errors).unwrap();
        assert_eq!(partial.completeness, ScanCompleteness::Partial);
        assert!(partial.partial && !partial.cancelled);

        let mut suppressed = v1();
        suppressed["suppressed_errors"] = json!(1);
        let suppressed: ScanReport = serde_json::from_value(suppressed).unwrap();
        assert_eq!(suppressed.completeness, ScanCompleteness::Partial);

        let mut finding_partial = v1();
        finding_partial["findings"] = json!([{
            "rule_id": "fixture",
            "rule_name": "fixture",
            "category": "developer",
            "risk": "safe",
            "recreate": "automatic",
            "path": "/cache",
            "size_bytes": 1,
            "file_count": 1,
            "last_modified": null,
            "description": "fixture",
            "partial": true
        }]);
        let finding_partial: ScanReport = serde_json::from_value(finding_partial).unwrap();
        assert_eq!(finding_partial.completeness, ScanCompleteness::Partial);

        let mut cancelled = v1();
        cancelled["partial"] = json!(false);
        cancelled["cancelled"] = json!(true);
        let cancelled: ScanReport = serde_json::from_value(cancelled).unwrap();
        assert_eq!(cancelled.completeness, ScanCompleteness::Cancelled);
        assert!(cancelled.partial && cancelled.cancelled);
    }

    #[test]
    fn v2_scope_and_completeness_serialize_as_stable_contract_values() {
        let scope = ScanScope {
            root_kind: ScanRootKind::Home,
            comparable: false,
            categories: vec![Category::Developer],
            max_risk: Some(Risk::Medium),
            min_size_bytes: 1024,
            rule_set_digest: TEST_RULE_SET_DIGEST.into(),
        };
        let json = serde_json::to_value(ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: 0,
            total_bytes: 0,
            total_files: 0,
            findings: Vec::new(),
            errors: Vec::new(),
            completeness: ScanCompleteness::Complete,
            scope: Some(scope.clone()),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
            cancelled: false,
            suppressed_errors: 0,
        })
        .unwrap();
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["completeness"], "complete");
        assert_eq!(json["scope"]["root_kind"], "home");
        assert_eq!(json["scope"]["comparable"], false);
        assert_eq!(json["scope"]["categories"][0], "developer");
        assert_eq!(json["scope"]["max_risk"], "medium");
        assert_eq!(json["scope"]["min_size_bytes"], 1024);
        assert_eq!(json["scope"]["rule_set_digest"], TEST_RULE_SET_DIGEST);
        assert_eq!(json["partial"], false);
        assert_eq!(json["cancelled"], false);
        assert_eq!(
            serde_json::from_value::<ScanScope>(json["scope"].clone()).unwrap(),
            scope
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_finding_paths_serialize_lossily_with_an_explicit_marker() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let native_path = PathBuf::from(OsString::from_vec(b"/cache/\xff".to_vec()));
        let finding = Finding {
            rule_id: "fixture".into(),
            rule_name: "fixture".into(),
            category: Category::Developer,
            risk: Risk::Safe,
            recreate: Recreate::Automatic,
            path: native_path.clone(),
            // Deliberately inconsistent: serialization must derive LossyUtf8
            // whenever the native path itself is not UTF-8.
            path_encoding: PathEncoding::Utf8,
            size_bytes: 1,
            file_count: 1,
            last_modified: None,
            description: String::new(),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
        };
        let report = ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: 0,
            total_bytes: 1,
            total_files: 1,
            findings: vec![finding],
            errors: Vec::new(),
            completeness: ScanCompleteness::Complete,
            scope: Some(ScanScope {
                root_kind: ScanRootKind::Custom,
                comparable: false,
                categories: Vec::new(),
                max_risk: None,
                min_size_bytes: 0,
                rule_set_digest: TEST_RULE_SET_DIGEST.into(),
            }),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
            cancelled: false,
            suppressed_errors: 0,
        };

        let json = serde_json::to_value(&report).unwrap();
        let display_path = native_path.to_string_lossy().into_owned();
        assert_eq!(json["findings"][0]["path"], display_path);
        assert_eq!(json["findings"][0]["path_encoding"], "lossy_utf8");

        let imported: ScanReport = serde_json::from_value(json).unwrap();
        assert_eq!(imported.findings[0].path_encoding, PathEncoding::LossyUtf8);
        assert_eq!(imported.findings[0].path.to_string_lossy(), display_path);
        assert_ne!(imported.findings[0].path, native_path);
    }

    #[test]
    fn v2_u64_values_above_javascript_safe_integer_serialize_as_decimal_strings() {
        let large = JS_SAFE_INTEGER_MAX + 1;
        let scope = ScanScope {
            root_kind: ScanRootKind::Home,
            comparable: false,
            categories: Vec::new(),
            max_risk: None,
            min_size_bytes: large,
            rule_set_digest: TEST_RULE_SET_DIGEST.into(),
        };
        let report = ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: 0,
            total_bytes: large,
            total_files: large,
            findings: vec![Finding {
                rule_id: "large".into(),
                rule_name: "large".into(),
                category: Category::Other,
                risk: Risk::Safe,
                recreate: Recreate::Unknown,
                path: PathBuf::from("/cache/large"),
                path_encoding: PathEncoding::Utf8,
                size_bytes: large,
                file_count: large,
                last_modified: Some(large),
                description: String::new(),
                byte_accounting: ByteAccounting::Logical,
                partial: false,
            }],
            errors: Vec::new(),
            completeness: ScanCompleteness::Partial,
            scope: Some(scope),
            byte_accounting: ByteAccounting::Logical,
            partial: true,
            cancelled: false,
            suppressed_errors: large,
        };

        let json = serde_json::to_value(&report).unwrap();
        let decimal = large.to_string();
        assert_eq!(json["scope"]["min_size_bytes"], decimal);
        assert_eq!(json["total_bytes"], decimal);
        assert_eq!(json["total_files"], decimal);
        assert_eq!(json["scanned_at"], 0);
        assert_eq!(json["suppressed_errors"], decimal);
        assert_eq!(json["findings"][0]["size_bytes"], decimal);
        assert_eq!(json["findings"][0]["file_count"], decimal);
        assert_eq!(json["findings"][0]["last_modified"], decimal);
        assert!(json["findings"][0].get("path_encoding").is_none());

        let round_tripped: ScanReport = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped.scope.as_ref().unwrap().min_size_bytes, large);
        assert_eq!(round_tripped.total_bytes, large);
        assert_eq!(round_tripped.findings[0].size_bytes, large);
        assert_eq!(round_tripped.findings[0].last_modified, Some(large));
    }

    #[test]
    fn scanned_at_stays_a_safe_json_number_for_preview_v2() {
        let mut report = ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: JS_SAFE_INTEGER_MAX,
            total_bytes: 0,
            total_files: 0,
            findings: Vec::new(),
            errors: Vec::new(),
            completeness: ScanCompleteness::Complete,
            scope: Some(ScanScope {
                root_kind: ScanRootKind::Home,
                comparable: false,
                categories: Vec::new(),
                max_risk: None,
                min_size_bytes: 0,
                rule_set_digest: TEST_RULE_SET_DIGEST.into(),
            }),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
            cancelled: false,
            suppressed_errors: 0,
        };

        let serialized = serde_json::to_value(&report).unwrap();
        assert_eq!(serialized["scanned_at"], JS_SAFE_INTEGER_MAX);
        let mut too_large = serialized;
        too_large["scanned_at"] = json!(JS_SAFE_INTEGER_MAX + 1);
        assert!(serde_json::from_value::<ScanReport>(too_large).is_err());

        report.scanned_at = JS_SAFE_INTEGER_MAX + 1;
        assert!(serde_json::to_value(report).is_err());
    }

    #[test]
    fn public_reports_are_not_pairwise_comparable_without_private_root_identity() {
        fn report(scope: ScanScope, platform: &str) -> ScanReport {
            ScanReport {
                schema_version: 2,
                platform: platform.into(),
                scanned_at: 0,
                total_bytes: 0,
                total_files: 0,
                findings: Vec::new(),
                errors: Vec::new(),
                completeness: ScanCompleteness::Complete,
                scope: Some(scope),
                byte_accounting: ByteAccounting::Logical,
                partial: false,
                cancelled: false,
                suppressed_errors: 0,
            }
        }

        let scope = ScanScope {
            root_kind: ScanRootKind::Home,
            comparable: false,
            categories: vec![Category::Developer],
            max_risk: Some(Risk::Medium),
            min_size_bytes: 1024,
            rule_set_digest: TEST_RULE_SET_DIGEST.into(),
        };
        let baseline = report(scope.clone(), "linux");
        let same = report(scope.clone(), "linux");
        assert!(!baseline.is_comparable_with(&same));

        let mut changed_filters = scope.clone();
        changed_filters.min_size_bytes += 1;
        assert_ne!(scope, changed_filters);

        let mut changed_rules = scope.clone();
        changed_rules.rule_set_digest =
            "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into();
        assert_ne!(scope, changed_rules);
        assert!(!baseline.is_comparable_with(&report(scope.clone(), "windows")));

        let mut partial = same;
        partial.completeness = ScanCompleteness::Partial;
        partial.partial = true;
        assert!(!baseline.is_comparable_with(&partial));
    }

    #[test]
    fn v2_deserialization_rejects_contradictory_status_and_evidence() {
        let mut contradictory = json!({
            "schema_version": 2,
            "platform": "linux",
            "scanned_at": 0,
            "total_bytes": 0,
            "total_files": 0,
            "findings": [],
            "errors": [{"path": "/cache", "message": "unreadable"}],
            "completeness": "complete",
            "scope": {
                "root_kind": "home",
                "comparable": false,
                "categories": [],
                "max_risk": null,
                "min_size_bytes": 0,
                "rule_set_digest": TEST_RULE_SET_DIGEST
            }
        });
        assert!(serde_json::from_value::<ScanReport>(contradictory.clone()).is_err());

        contradictory["errors"] = json!([]);
        contradictory["partial"] = json!(true);
        assert!(serde_json::from_value::<ScanReport>(contradictory).is_err());
    }

    #[test]
    fn unsupported_report_schema_versions_are_rejected_on_read_and_write() {
        let report = ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: 0,
            total_bytes: 0,
            total_files: 0,
            findings: Vec::new(),
            errors: Vec::new(),
            completeness: ScanCompleteness::Complete,
            scope: Some(ScanScope {
                root_kind: ScanRootKind::Home,
                comparable: false,
                categories: Vec::new(),
                max_risk: None,
                min_size_bytes: 0,
                rule_set_digest: TEST_RULE_SET_DIGEST.into(),
            }),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
            cancelled: false,
            suppressed_errors: 0,
        };
        let serialized = serde_json::to_value(&report).unwrap();

        for version in [0, 3, u32::MAX] {
            let mut input = serialized.clone();
            input["schema_version"] = json!(version);
            assert!(
                serde_json::from_value::<ScanReport>(input).is_err(),
                "schema version {version} must be rejected"
            );

            let mut output = report.clone();
            output.schema_version = version;
            assert!(
                serde_json::to_value(output).is_err(),
                "schema version {version} must not be serialized"
            );
        }

        let mut unknown_v1 = report.clone();
        unknown_v1.schema_version = 1;
        unknown_v1.completeness = ScanCompleteness::Unknown;
        unknown_v1.scope = None;
        assert!(serde_json::to_value(unknown_v1).is_err());

        let mut unsafe_v1 = report;
        unsafe_v1.schema_version = 1;
        unsafe_v1.scope = None;
        unsafe_v1.total_bytes = JS_SAFE_INTEGER_MAX + 1;
        assert!(serde_json::to_value(unsafe_v1).is_err());
    }

    #[test]
    fn v2_serialization_rejects_inconsistent_legacy_aliases() {
        let mut report = ScanReport {
            schema_version: 2,
            platform: "linux".into(),
            scanned_at: 0,
            total_bytes: 0,
            total_files: 0,
            findings: Vec::new(),
            errors: Vec::new(),
            completeness: ScanCompleteness::Complete,
            scope: Some(ScanScope {
                root_kind: ScanRootKind::Home,
                comparable: false,
                categories: Vec::new(),
                max_risk: None,
                min_size_bytes: 0,
                rule_set_digest: TEST_RULE_SET_DIGEST.into(),
            }),
            byte_accounting: ByteAccounting::Logical,
            partial: false,
            cancelled: false,
            suppressed_errors: 0,
        };
        assert!(serde_json::to_value(&report).is_ok());
        report.partial = true;
        assert!(serde_json::to_value(&report).is_err());
    }
}
