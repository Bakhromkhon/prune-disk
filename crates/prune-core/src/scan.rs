use crate::control::{ErrorLog, Progress};
use crate::discovery::discover;
use crate::measurement::{measure, unix_secs};
use crate::model::{ByteAccounting, Category, Risk, ScanReport};
use crate::rules::ValidatedRuleSet;
use crate::{ScanControl, ScanProgress, ScanStage};
use rayon::prelude::*;
use std::path::Path;
use std::sync::Mutex;
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
    scan_with_control(options, &ScanControl::default(), |_| {})
}

/// Per-scan bounded workers; never uses the global Rayon pool. Callbacks are
/// serialized, synchronous, and must not block. Cancellation is cooperative:
/// an in-flight filesystem operation cannot be interrupted.
pub fn scan_with_control(
    options: &ScanOptions,
    control: &ScanControl,
    on_progress: impl Fn(ScanProgress) + Send + Sync,
) -> ScanReport {
    let progress = Progress::new(control, &on_progress);
    progress.stage(ScanStage::Discovering);
    let mut errors = ErrorLog::default();
    let disjoint = discover(&options.rules, &options.os, &mut errors, &progress);
    let disjoint: Vec<_> = disjoint
        .into_iter()
        .filter(|(rule, _)| {
            (options.categories.is_empty() || options.categories.contains(&rule.category))
                && options.max_risk.is_none_or(|max| rule.risk <= max)
        })
        .collect();
    let results = Mutex::new((Vec::new(), errors));
    if !control.is_cancelled() {
        progress.stage(ScanStage::Measuring);
        if !control.is_cancelled() && !disjoint.is_empty() {
            match rayon::ThreadPoolBuilder::new()
                .num_threads(control.worker_count().min(disjoint.len()))
                .build()
            {
                Ok(pool) => pool.install(|| {
                    disjoint.par_iter().for_each(|(rule, path)| {
                        if control.is_cancelled() {
                            return;
                        }
                        let measurement = measure(rule, path, options.min_size, &progress);
                        let mut results = results.lock().unwrap();
                        results.1.append(measurement.errors);
                        if let Some(finding) = measurement.finding {
                            results.0.push(finding);
                        }
                    })
                }),
                Err(error) => record_error(&mut results.lock().unwrap().1, Path::new(""), error),
            }
        }
    }
    let (mut findings, mut errors) = results.into_inner().unwrap();
    findings.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
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
    let cancelled = control.is_cancelled();
    progress.stage(if cancelled {
        ScanStage::Cancelled
    } else {
        ScanStage::Completed
    });
    errors
        .errors
        .sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.message.cmp(&b.message)));
    errors
        .errors
        .dedup_by(|a, b| a.path == b.path && a.message == b.message);
    ScanReport {
        schema_version: 1,
        platform: options.os.clone(),
        scanned_at: unix_secs(Some(SystemTime::now())).unwrap_or(0),
        total_bytes,
        total_files,
        partial: cancelled || !errors.is_empty(),
        cancelled,
        suppressed_errors: errors.suppressed,
        byte_accounting: ByteAccounting::Logical,
        findings,
        errors: errors.errors,
    }
}

pub(crate) fn record_error(errors: &mut ErrorLog, path: &Path, error: impl ToString) {
    errors.record(path, error);
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
    fn pre_cancelled_scan_has_only_cancelled_terminal_and_no_findings() {
        let control = ScanControl::default();
        control.clone().cancel();
        let events = Mutex::new(Vec::new());
        let report = scan_with_control(&ScanOptions::default(), &control, |p| {
            events.lock().unwrap().push(p);
        });
        assert!(report.cancelled && report.partial);
        assert!(report.findings.is_empty());
        let events = events.into_inner().unwrap();
        assert_eq!(events.last().unwrap().stage, ScanStage::Cancelled);
        assert!(!events.iter().any(|p| p.stage == ScanStage::Measuring));
        assert!(events.iter().all(|p| p.files_measured == 0));
    }

    #[test]
    fn cancel_after_discovery_work_does_not_measure_partial_risk_resolution() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache/child")).unwrap();
        fs::write(dir.path().join("app/cache/child/file"), b"123").unwrap();
        let opts = options(
            dir.path(),
            vec![
                rule("safe", "~/app/cache", Risk::Safe),
                rule("never", "~/app/cache/child", Risk::Never),
            ],
        );
        let control = ScanControl::default();
        let events = Mutex::new(Vec::new());
        let report = scan_with_control(&opts, &control, |p| {
            if p.stage == ScanStage::Discovering && p.candidates > 0 {
                control.cancel();
            }
            events.lock().unwrap().push(p);
        });
        assert!(report.cancelled && report.partial);
        assert!(report.findings.is_empty());
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.stage == ScanStage::Measuring)
        );
        assert!(dir.path().join("app/cache/child/file").is_file());
    }

    #[test]
    fn measurement_cancellation_returns_explicit_partial_observed_work() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("app/cache");
        fs::create_dir_all(&cache).unwrap();
        for i in 0..100 {
            fs::write(cache.join(i.to_string()), b"123").unwrap();
        }
        let opts = options(dir.path(), vec![rule("cache", "~/app/cache", Risk::Safe)]);
        let control = ScanControl::default().with_worker_count(1);
        let events = Mutex::new(Vec::new());
        let report = scan_with_control(&opts, &control, |p| {
            if p.stage == ScanStage::Measuring && p.files_measured > 0 {
                control.cancel();
            }
            events.lock().unwrap().push(p);
        });
        assert!(report.cancelled && report.partial);
        assert_eq!(report.total_files, 1);
        assert_eq!(report.total_bytes, 3);
        assert!(report.findings[0].partial);
        let events = events.into_inner().unwrap();
        let last = events.last().unwrap();
        assert_eq!(last.stage, ScanStage::Cancelled);
        assert_eq!(last.files_measured, 1);
        assert_eq!(last.bytes_measured, 3);
        assert!(events.len() <= 6);
        assert_eq!(fs::read_dir(cache).unwrap().count(), 100);
    }

    #[test]
    fn worker_limits_clamp_and_preserve_results() {
        assert_eq!(ScanControl::default().worker_count(), 2);
        assert_eq!(
            ScanControl::default().with_worker_count(0).worker_count(),
            1
        );
        assert_eq!(
            ScanControl::default()
                .with_worker_count(usize::MAX)
                .worker_count(),
            crate::MAX_SCAN_WORKERS
        );
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        for i in 0..12 {
            fs::write(dir.path().join(format!("app/cache/{i}")), b"123").unwrap();
        }
        let opts = options(dir.path(), vec![rule("cache", "~/app/cache/*", Risk::Safe)]);
        for workers in [1, 2, crate::MAX_SCAN_WORKERS] {
            let report = scan_with_control(
                &opts,
                &ScanControl::default().with_worker_count(workers),
                |_| {},
            );
            assert!(!report.partial && !report.cancelled);
            assert_eq!(report.total_files, 12);
            assert_eq!(report.total_bytes, 36);
        }
    }

    #[test]
    fn candidate_limit_discards_entire_discovery_fail_closed() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        for i in 0..17 {
            fs::write(dir.path().join(format!("app/cache/{i}")), b"x").unwrap();
        }
        let report = scan_with_control(
            &options(dir.path(), vec![rule("cache", "~/app/cache/*", Risk::Safe)]),
            &ScanControl::default().with_candidate_limit(16),
            |_| {},
        );
        assert!(report.partial);
        assert!(!report.cancelled);
        assert!(report.findings.is_empty());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.message.contains("candidate limit"))
        );
    }

    #[test]
    fn two_thousand_disjoint_candidates_do_not_retain_identity_handles() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        for i in 0..2_000 {
            let candidate = dir.path().join(format!("app/cache/{i}"));
            fs::create_dir(&candidate).unwrap();
            fs::write(candidate.join("file"), b"x").unwrap();
        }
        let report = scan(&options(
            dir.path(),
            vec![rule("cache", "~/app/cache/*", Risk::Safe)],
        ));
        assert!(!report.partial, "{:?}", report.errors);
        assert_eq!(report.findings.len(), 2_000);
        assert_eq!(report.total_files, 2_000);
        assert_eq!(report.total_bytes, 2_000);
    }

    #[test]
    fn failed_never_descendant_expansion_cannot_leave_a_safe_parent() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/local"), b"x").unwrap();
        for rules in [
            vec![
                rule("parent", "~/app/cache", Risk::Safe),
                rule("never", "~/app/cache/local/*", Risk::Never),
            ],
            vec![
                rule("never", "~/app/cache/local/*", Risk::Never),
                rule("parent", "~/app/cache", Risk::Safe),
            ],
        ] {
            let mut opts = options(dir.path(), rules);
            opts.max_risk = Some(Risk::Safe);
            let report = scan(&opts);
            assert!(report.partial);
            assert!(report.findings.is_empty());
            assert!(!report.errors.is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn discovery_errors_are_bounded_and_suppression_is_reported() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let cache = dir.path().join("app/cache");
        fs::create_dir_all(&cache).unwrap();
        for i in 0..crate::MAX_SCAN_ERRORS + 20 {
            symlink("missing", cache.join(i.to_string())).unwrap();
        }
        let report = scan(&options(
            dir.path(),
            vec![rule("cache", "~/app/cache/*", Risk::Safe)],
        ));
        assert!(report.partial);
        assert_eq!(report.errors.len(), crate::MAX_SCAN_ERRORS);
        assert_eq!(report.suppressed_errors, 20);
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

    #[test]
    fn hardlinked_candidate_aliases_merge_metadata_but_disjoint_directories_stay_logical() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/a")).unwrap();
        fs::create_dir_all(dir.path().join("app/b")).unwrap();
        fs::write(dir.path().join("app/a/file"), b"1234").unwrap();
        fs::hard_link(dir.path().join("app/a/file"), dir.path().join("app/b/file")).unwrap();
        let mut safe = rule("safe", "~/app/a/file", Risk::Safe);
        let mut never = rule("never", "~/app/b/file", Risk::Never);
        never.recreate = Recreate::Manual;
        for rules in [
            vec![safe.clone(), never.clone()],
            vec![never.clone(), safe.clone()],
        ] {
            let mut opts = options(dir.path(), rules);
            let report = scan(&opts);
            assert!(!report.partial, "{:?}", report.errors);
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.total_bytes, 4);
            assert_eq!(report.findings[0].risk, Risk::Never);
            assert_eq!(report.findings[0].recreate, Recreate::Unknown);
            opts.max_risk = Some(Risk::Safe);
            assert!(scan(&opts).findings.is_empty());
        }
        safe.paths = vec!["~/app/a".into()];
        never.paths = vec!["~/app/b".into()];
        let report = scan(&options(dir.path(), vec![safe, never]));
        assert!(!report.partial, "{:?}", report.errors);
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.total_bytes, 8);
        assert_eq!(report.total_files, 2);
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
