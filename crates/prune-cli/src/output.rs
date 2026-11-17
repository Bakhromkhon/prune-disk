use crate::util::{display_path, truncate};
use humansize::{DECIMAL, format_size};
use prune_core::{Rule, ScanReport};

pub fn print_report(report: &ScanReport) {
    println!("Prune {} — read-only scan", env!("CARGO_PKG_VERSION"));
    println!(
        "Platform: {} · Findings: {} · Files: {} · Total: {}",
        report.platform,
        report.findings.len(),
        report.total_files,
        format_size(report.total_bytes, DECIMAL)
    );
    if !report.errors.is_empty() {
        println!("Warnings: {} (use --json to inspect)", report.errors.len());
    }
    println!();

    if report.findings.is_empty() {
        println!(
            "Nothing found. Your disk looks tidy, or the built-in rules do not cover this machine yet."
        );
        return;
    }

    println!(
        "{:<10} {:<7} {:<11} {:<30} PATH",
        "SIZE", "RISK", "CATEGORY", "NAME"
    );
    for finding in &report.findings {
        println!(
            "{:<10} {:<7} {:<11} {:<30} {}",
            format_size(finding.size_bytes, DECIMAL),
            finding.risk.as_str(),
            finding.category.as_str(),
            truncate(&finding.rule_name, 30),
            display_path(&finding.path)
        );
    }
}

pub fn print_rules(rules: &[Rule]) {
    println!(
        "{:<24} {:<7} {:<11} {:<18} PATHS",
        "ID", "RISK", "CATEGORY", "OS"
    );
    for rule in rules {
        println!(
            "{:<24} {:<7} {:<11} {:<18} {}",
            truncate(&rule.id, 24),
            rule.risk.as_str(),
            rule.category.as_str(),
            rule.os.join(","),
            rule.paths.join(", ")
        );
    }
}
