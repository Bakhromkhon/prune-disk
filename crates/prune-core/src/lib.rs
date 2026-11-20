#![forbid(unsafe_code)]
#![doc = "Core scanning and rule engine for Prune."]

mod control;
mod discovery;
mod measurement;
pub mod model;
mod path_policy;
mod plan;
pub mod rules;
pub mod scan;

pub use control::{
    MAX_SCAN_CANDIDATES, MAX_SCAN_ERRORS, MAX_SCAN_WORKERS, ScanControl, ScanProgress, ScanStage,
};
pub use model::{
    ByteAccounting, Category, Finding, PathEncoding, Recreate, Risk, Rule, ScanCompleteness,
    ScanError, ScanReport, ScanRootKind, ScanScope,
};
pub use plan::{CleanupPlan, PlanItem, build_plan};
pub use rules::{
    RuleError, ValidatedRuleSet, builtin_rules, load_rules_from_dir, validate_path_pattern,
};
pub use scan::{ScanOptions, scan, scan_with_control};
