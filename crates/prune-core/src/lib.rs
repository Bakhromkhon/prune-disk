#![forbid(unsafe_code)]
#![doc = "Core scanning and rule engine for Prune."]

mod discovery;
mod measurement;
pub mod model;
mod path_policy;
pub mod rules;
pub mod scan;

pub use model::{ByteAccounting, Category, Finding, Recreate, Risk, Rule, ScanError, ScanReport};
pub use rules::{
    RuleError, ValidatedRuleSet, builtin_rules, load_rules_from_dir, validate_path_pattern,
};
pub use scan::{ScanOptions, scan};
