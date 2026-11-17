#![forbid(unsafe_code)]
#![doc = "Core scanning and rule engine for Prune."]

pub mod model;
pub mod rules;
pub mod scan;

pub use model::{Category, Finding, Recreate, Risk, Rule, ScanError, ScanReport};
pub use rules::{RuleError, builtin_rules, load_rules_from_dir, validate_path_pattern};
pub use scan::{ScanOptions, scan};
