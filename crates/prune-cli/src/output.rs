use crate::util::{display_path, escape_terminal, truncate};
use humansize::{DECIMAL, format_size};
use prune_core::{Rule, ScanReport};

pub fn print_report(report: &ScanReport) {
    println!("Prune {} — read-only scan", env!("CARGO_PKG_VERSION"));
    println!(
        "Platform: {} · Findings: {} · Files: {} · Logical size: {}",
        escape_terminal(&report.platform),
        report.findings.len(),
        report.total_files,
        format_size(report.total_bytes, DECIMAL)
    );
    println!(
        "Status: {} · Logical size is not a promise of reclaimable space.",
        if report.cancelled {
            "cancelled (partial; exit code 130)"
        } else if report.partial {
            "partial"
        } else {
            "complete for selected rules"
        }
    );
    if report.suppressed_errors > 0 {
        println!(
            "Additional warnings suppressed: {}",
            report.suppressed_errors
        );
    }
    if !report.errors.is_empty() {
        println!("Warnings: {}", report.errors.len());
        for error in &report.errors {
            println!(
                "  {}: {}",
                escape_terminal(&error.path),
                escape_terminal(&error.message)
            );
        }
    }
    println!();

    if report.findings.is_empty() {
        println!(
            "No findings matched the scanned rules. This does not mean the disk is clean; coverage and access may be limited."
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
            truncate(&escape_terminal(&finding.rule_name), 30),
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
            truncate(&escape_terminal(&rule.id), 24),
            rule.risk.as_str(),
            rule.category.as_str(),
            escape_terminal(&rule.os.join(",")),
            escape_terminal(&rule.paths.join(", "))
        );
    }
}
