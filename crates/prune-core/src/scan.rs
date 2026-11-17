use crate::discovery::discover;
use crate::measurement::{Measurement, measure, unix_secs};
use crate::model::{ByteAccounting, Category, Risk, ScanError, ScanReport};
use crate::rules::ValidatedRuleSet;
use rayon::prelude::*;
use std::path::Path;
use std::time::SystemTime;

/// Options for a read-only scan. Unvalidated rules cannot enter the scanner.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub os: String,
    pub rules: ValidatedRuleSet,
    pub min_size: u64,
    pub categories: Vec<Category>,
    pub max_risk: Option<Risk>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            rules: ValidatedRuleSet::empty(),
            min_size: 0,
            categories: Vec::new(),
            max_risk: None,
        }
    }
}

/// Read-only, best-effort logical file measurement.
///
/// Components are checked with symlink_metadata before traversal. This is NOT
/// a race-free filesystem sandbox: another process can replace an ancestor
/// between a check and a read. No findings authorize later deletion.
pub fn scan(options: &ScanOptions) -> ScanReport {
    let mut errors = Vec::new();
    let disjoint = discover(&options.rules, &options.os, &mut errors);
    let measurements: Vec<Measurement> = disjoint
        .par_iter()
        .filter(|(rule, _)| {
            (options.categories.is_empty() || options.categories.contains(&rule.category))
                && options.max_risk.is_none_or(|max| rule.risk <= max)
        })
        .map(|(rule, path)| measure(rule, path, options.min_size))
        .collect();
    let mut findings = Vec::new();
    for measurement in measurements {
        errors.extend(measurement.errors);
        if let Some(finding) = measurement.finding {
            findings.push(finding);
        }
    }
    findings.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    errors.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.message.cmp(&b.message)));
    errors.dedup_by(|a, b| a.path == b.path && a.message == b.message);
    let mut total_bytes = 0_u64;
    let mut total_files = 0_u64;
    for finding in &findings {
        match (
            total_bytes.checked_add(finding.size_bytes),
            total_files.checked_add(finding.file_count),
        ) {
            (Some(bytes), Some(files)) => {
                total_bytes = bytes;
                total_files = files;
            }
            _ => record_error(&mut errors, &finding.path, "scan totals overflow"),
        }
    }
    ScanReport {
        schema_version: 1,
        platform: options.os.clone(),
        scanned_at: unix_secs(Some(SystemTime::now())).unwrap_or(0),
        total_bytes,
        total_files,
        partial: !errors.is_empty(),
        byte_accounting: ByteAccounting::Logical,
        findings,
        errors,
    }
}

pub(crate) fn record_error(errors: &mut Vec<ScanError>, path: &Path, error: impl ToString) {
    errors.push(ScanError {
        path: path.display().to_string(),
        message: error.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Recreate, Rule};
    use std::fs;
    use tempfile::tempdir;

    fn rule(id: &str, path: &str, risk: Risk) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            description: "fixture".into(),
            os: vec![std::env::consts::OS.into()],
            category: Category::Developer,
            risk,
            recreate: Recreate::Automatic,
            paths: vec![path.into()],
        }
    }

    fn options(root: &Path, rules: Vec<Rule>) -> ScanOptions {
        ScanOptions {
            rules: ValidatedRuleSet::for_trusted_root(rules, root).unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn recursive_logical_measurement_is_read_only_and_honors_minimum() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("app/cache");
        fs::create_dir_all(cache.join("nested")).unwrap();
        fs::write(cache.join("a"), b"hello").unwrap();
        fs::write(cache.join("nested/b"), b"world!").unwrap();
        let before = fs::metadata(cache.join("a")).unwrap().modified().unwrap();
        let mut opts = options(dir.path(), vec![rule("fixture", "~/app/cache", Risk::Safe)]);
        let report = scan(&opts);
        assert_eq!(report.total_bytes, 11);
        assert_eq!(report.total_files, 2);
        assert_eq!(report.byte_accounting, ByteAccounting::Logical);
        assert!(!report.partial);
        assert_eq!(fs::read(cache.join("a")).unwrap(), b"hello");
        assert_eq!(
            fs::metadata(cache.join("a")).unwrap().modified().unwrap(),
            before
        );
        opts.min_size = 12;
        assert!(scan(&opts).is_empty());
    }

    #[test]
    fn overlaps_are_disjoint_order_independent_and_conservative() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache/child")).unwrap();
        fs::write(dir.path().join("app/cache/child/file"), b"1234").unwrap();
        let rules = vec![
            rule("parent", "~/app/cache", Risk::Safe),
            rule("child", "~/app/cache/child", Risk::Never),
            rule("duplicate", "~/app/cache", Risk::Low),
        ];
        for rules in [rules.clone(), rules.into_iter().rev().collect()] {
            let mut opts = options(dir.path(), rules);
            let report = scan(&opts);
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.total_bytes, 4);
            assert_eq!(report.findings[0].risk, Risk::Never);
            assert_eq!(report.findings[0].rule_id, "duplicate");
            opts.max_risk = Some(Risk::Safe);
            assert!(scan(&opts).is_empty());
        }
    }

    #[test]
    fn terminal_wildcard_only_lists_immediate_children() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache/nested")).unwrap();
        fs::write(dir.path().join("app/cache/nested/file"), b"123").unwrap();
        fs::write(dir.path().join("app/cache/plain"), b"45").unwrap();
        let report = scan(&options(
            dir.path(),
            vec![rule("fixture", "~/app/cache/*", Risk::Safe)],
        ));
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.total_bytes, 5);
    }

    #[test]
    fn absolute_fixture_and_missing_paths() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/file"), b"abc").unwrap();
        let absolute = dir
            .path()
            .join("app/cache")
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let report = scan(&options(
            dir.path(),
            vec![
                rule("absolute", &absolute, Risk::Safe),
                rule("missing", "~/app/missing", Risk::Safe),
            ],
        ));
        assert_eq!(report.total_bytes, 3);
        assert!(!report.partial);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_ancestors_literal_glob_root_and_nested_links() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir_all(outside.path().join("cache")).unwrap();
        fs::write(outside.path().join("cache/secret"), b"secret").unwrap();
        symlink(outside.path(), dir.path().join("linked")).unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/local"), b"1234").unwrap();
        symlink(outside.path(), dir.path().join("app/cache/linked")).unwrap();
        for pattern in ["~/linked/cache", "~/linked/cache/*", "~/app/cache/linked"] {
            let report = scan(&options(
                dir.path(),
                vec![rule("fixture", pattern, Risk::Safe)],
            ));
            assert_eq!(report.total_bytes, 0, "{pattern}");
            assert!(report.partial, "{pattern}");
        }
        for pattern in ["~/app/cache", "~/app/cache/*"] {
            let report = scan(&options(
                dir.path(),
                vec![rule("fixture", pattern, Risk::Safe)],
            ));
            assert_eq!(report.total_bytes, 4, "{pattern}");
            assert!(report.partial, "{pattern}");
            if pattern == "~/app/cache" {
                assert!(report.findings[0].partial);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn hardlinks_are_explicitly_logical_not_unique_reclaimable_bytes() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("app/cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("a"), b"1234").unwrap();
        fs::hard_link(cache.join("a"), cache.join("b")).unwrap();
        let report = scan(&options(
            dir.path(),
            vec![rule("fixture", "~/app/cache", Risk::Safe)],
        ));
        assert_eq!(report.total_bytes, 8);
        assert_eq!(report.total_files, 2);
        assert_eq!(report.byte_accounting, ByteAccounting::Logical);
    }

    fn case_insensitive_fixture() -> Option<tempfile::TempDir> {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/local"), b"1234").unwrap();
        let alias = dir.path().join("app/CACHE/LOCAL");
        match fs::symlink_metadata(&alias) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("SKIP case-alias regression: temporary filesystem is case-sensitive");
                None
            }
            Err(error) => panic!("cannot inspect case-alias fixture: {error}"),
            Ok(_) => {
                let original = dir.path().join("app/cache/local");
                assert!(same_file::is_same_file(&original, &alias).unwrap());
                eprintln!(
                    "case-alias fixture: canonicalize normalizes stored case = {}",
                    fs::canonicalize(&original).unwrap() == fs::canonicalize(&alias).unwrap()
                );
                Some(dir)
            }
        }
    }

    #[test]
    fn case_alias_descendant_never_risk_propagates_before_filtering() {
        let Some(dir) = case_insensitive_fixture() else {
            return;
        };
        let rules = vec![
            rule("parent", "~/app/cache", Risk::Safe),
            rule("descendant", "~/app/CACHE/local", Risk::Never),
        ];
        for rules in [rules.clone(), rules.into_iter().rev().collect()] {
            let mut opts = options(dir.path(), rules);
            let report = scan(&opts);
            assert!(!report.partial, "{:?}", report.errors);
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.total_bytes, 4);
            assert_eq!(report.findings[0].risk, Risk::Never);
            opts.max_risk = Some(Risk::Safe);
            let report = scan(&opts);
            assert!(report.findings.is_empty());
            assert_eq!(report.total_bytes, 0);
            assert!(!report.partial, "{:?}", report.errors);
        }
    }

    #[test]
    fn equal_case_aliases_merge_conservatively_for_directories_files_and_globs() {
        let Some(dir) = case_insensitive_fixture() else {
            return;
        };
        for (safe, never) in [
            ("~/app/cache", "~/app/CACHE"),
            ("~/app/cache/local", "~/app/CACHE/LOCAL"),
            ("~/app/cache/*", "~/app/CACHE/LOCAL"),
        ] {
            let rules = vec![
                rule("safe", safe, Risk::Safe),
                rule("never", never, Risk::Never),
            ];
            for rules in [rules.clone(), rules.into_iter().rev().collect()] {
                let mut opts = options(dir.path(), rules);
                let report = scan(&opts);
                assert!(!report.partial, "{:?}", report.errors);
                assert_eq!(report.findings.len(), 1, "{safe}, {never}");
                assert_eq!(report.total_bytes, 4);
                assert_eq!(report.findings[0].risk, Risk::Never);
                opts.max_risk = Some(Risk::Safe);
                let report = scan(&opts);
                assert!(report.findings.is_empty());
                assert!(!report.partial, "{:?}", report.errors);
            }
        }
    }
}
