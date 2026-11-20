use crate::model::{
    ByteAccounting, Category, PathEncoding, Recreate, Risk, ScanCompleteness, ScanReport, ScanScope,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A read-only cleanup proposal derived from an immutable scan report.
///
/// A plan never authorizes deletion. `proposed_bytes` is a logical-size
/// estimate for proposed items only; it is not guaranteed reclaimable space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub schema_version: u32,
    pub platform: String,
    pub completeness: ScanCompleteness,
    pub proposed_count: usize,
    pub excluded_count: usize,
    pub requires_review_count: usize,
    #[serde(with = "crate::model::js_safe_u64")]
    pub proposed_bytes: u64,
    #[serde(with = "crate::model::js_safe_u64")]
    pub excluded_bytes: u64,
    #[serde(with = "crate::model::js_safe_u64")]
    pub report_total_bytes: u64,
    pub byte_accounting: ByteAccounting,
    pub scope: Option<ScanScope>,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
}

/// One candidate considered by the planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub rule_id: String,
    pub rule_name: String,
    pub category: Category,
    pub risk: Risk,
    pub recreate: Recreate,
    #[serde(with = "crate::model::display_path")]
    pub path: PathBuf,
    #[serde(skip_serializing_if = "PathEncoding::is_utf8")]
    pub path_encoding: PathEncoding,
    #[serde(with = "crate::model::js_safe_u64")]
    pub size_bytes: u64,
    #[serde(with = "crate::model::js_safe_u64")]
    pub file_count: u64,
    #[serde(with = "crate::model::js_safe_option_u64")]
    pub last_modified: Option<u64>,
    pub description: String,
    pub proposed: bool,
    pub requires_review: bool,
    pub reason: String,
    pub uncertainty: Vec<String>,
}

/// Build a deterministic, side-effect-free plan from a scan report.
pub fn build_plan(report: &ScanReport) -> CleanupPlan {
    let complete = report.completeness == ScanCompleteness::Complete;
    let mut items = Vec::with_capacity(report.findings.len());
    let mut proposed_count = 0usize;
    let mut excluded_count = 0usize;
    let mut requires_review_count = 0usize;
    let mut proposed_bytes = 0u64;
    let mut excluded_bytes = 0u64;

    for finding in &report.findings {
        let (proposed, reason) = if finding.risk == Risk::Never {
            (false, "excluded: rule risk is never")
        } else if !complete {
            (
                false,
                "excluded: scan is not complete; coverage may be incomplete",
            )
        } else {
            (true, "proposed by read-only policy")
        };

        let requires_review = finding.risk >= Risk::High;
        let mut uncertainty = Vec::new();
        if finding.partial {
            uncertainty.push("finding may be incomplete".to_string());
        }
        if !complete {
            uncertainty.push(format!(
                "scan completeness is {}",
                report.completeness.as_str()
            ));
        }
        if finding.last_modified.is_none() {
            uncertainty.push("modification time unavailable".to_string());
        }
        if finding.path.to_str().is_none() || finding.path_encoding != PathEncoding::Utf8 {
            uncertainty.push("path is display-only and cannot round-trip".to_string());
        }

        if proposed {
            proposed_count += 1;
            proposed_bytes = proposed_bytes.saturating_add(finding.size_bytes);
            if requires_review {
                requires_review_count += 1;
            }
        } else {
            excluded_count += 1;
            excluded_bytes = excluded_bytes.saturating_add(finding.size_bytes);
        }

        items.push(PlanItem {
            rule_id: finding.rule_id.clone(),
            rule_name: finding.rule_name.clone(),
            category: finding.category,
            risk: finding.risk,
            recreate: finding.recreate,
            path: finding.path.clone(),
            path_encoding: finding.path_encoding,
            size_bytes: finding.size_bytes,
            file_count: finding.file_count,
            last_modified: finding.last_modified,
            description: finding.description.clone(),
            proposed,
            requires_review,
            reason: reason.to_string(),
            uncertainty,
        });
    }

    // Findings are already sorted by scan; keep deterministic input order.
    let mut warnings = Vec::new();
    if !report.errors.is_empty() {
        warnings.push(format!(
            "{} scan warning(s) were reported",
            report.errors.len()
        ));
    }
    if report.suppressed_errors > 0 {
        warnings.push(format!(
            "{} diagnostic(s) were suppressed",
            report.suppressed_errors
        ));
    }
    if !complete {
        warnings.push(format!(
            "scan completeness is {}; proposed items are disabled",
            report.completeness.as_str()
        ));
    }

    CleanupPlan {
        schema_version: 1,
        platform: report.platform.clone(),
        completeness: report.completeness,
        proposed_count,
        excluded_count,
        requires_review_count,
        proposed_bytes,
        excluded_bytes,
        report_total_bytes: report.total_bytes,
        byte_accounting: report.byte_accounting,
        scope: report.scope.clone(),
        items,
        warnings,
    }
}

impl ScanCompleteness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}
