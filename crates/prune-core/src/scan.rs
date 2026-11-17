use crate::model::{Category, Finding, Risk, Rule, ScanError, ScanReport};
use glob::glob;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

/// Options for a read-only scan. No deletion happens here.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub os: String,
    pub rules: Vec<Rule>,
    pub min_size: u64,
    pub categories: Vec<Category>,
    pub max_risk: Option<Risk>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            rules: Vec::new(),
            min_size: 0,
            categories: Vec::new(),
            max_risk: None,
        }
    }
}

/// Scan every rule path that applies to the current OS.
///
/// The scan is read-only: it stats files, sums sizes and records errors, but
/// never opens user data for writing and never removes anything.
pub fn scan(options: &ScanOptions) -> ScanReport {
    let mut errors = Vec::new();
    let mut candidates: Vec<(Rule, PathBuf)> = Vec::new();
    let mut seen_paths = HashSet::new();

    for rule in &options.rules {
        if !rule.applies_to(&options.os) {
            continue;
        }
        if !options.categories.is_empty() && !options.categories.contains(&rule.category) {
            continue;
        }
        if let Some(max_risk) = options.max_risk {
            if rule.risk > max_risk {
                continue;
            }
        }

        for pattern in &rule.paths {
            let expanded = expand_home(pattern);
            match glob(&expanded) {
                Ok(paths) => {
                    for path in paths {
                        match path {
                            Ok(path) => {
                                if seen_paths.insert(path.clone()) {
                                    candidates.push((rule.clone(), path));
                                }
                            }
                            Err(error) => errors.push(ScanError {
                                path: expanded.clone(),
                                message: error.to_string(),
                            }),
                        }
                    }
                }
                Err(error) => errors.push(ScanError {
                    path: expanded.clone(),
                    message: error.to_string(),
                }),
            }
        }
    }

    let measurements: Vec<Measurement> = candidates
        .par_iter()
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

    let total_bytes = findings.iter().map(|finding| finding.size_bytes).sum();
    let total_files = findings.iter().map(|finding| finding.file_count).sum();

    ScanReport {
        schema_version: 1,
        platform: options.os.clone(),
        scanned_at: now_unix(),
        total_bytes,
        total_files,
        findings,
        errors,
    }
}

struct Measurement {
    finding: Option<Finding>,
    errors: Vec<ScanError>,
}

fn measure(rule: &Rule, path: &Path, min_size: u64) -> Measurement {
    let mut errors = Vec::new();
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            errors.push(ScanError {
                path: path.display().to_string(),
                message: error.to_string(),
            });
            return Measurement {
                finding: None,
                errors,
            };
        }
    };

    // Never follow symlinks. A rule that points at a link should not give
    // Prune access to its target, and a symlink inside a cache directory
    // should not make a directory look larger than it is.
    if metadata.file_type().is_symlink() {
        return Measurement {
            finding: None,
            errors,
        };
    }

    let (size_bytes, file_count, last_modified) = if metadata.is_dir() {
        let mut size = 0_u64;
        let mut count = 0_u64;
        let mut newest = 0_u64;

        for entry in WalkDir::new(path)
            .follow_links(false)
            .same_file_system(true)
        {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_symlink() {
                        continue;
                    }
                    if entry.file_type().is_file() {
                        match entry.metadata() {
                            Ok(metadata) => {
                                size += metadata.len();
                                count += 1;
                                if let Some(modified) = unix_secs(metadata.modified().ok()) {
                                    newest = newest.max(modified);
                                }
                            }
                            Err(error) => errors.push(ScanError {
                                path: entry.path().display().to_string(),
                                message: error.to_string(),
                            }),
                        }
                    }
                }
                Err(error) => errors.push(ScanError {
                    path: error
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| path.display().to_string()),
                    message: error.to_string(),
                }),
            }
        }

        (size, count, if newest == 0 { None } else { Some(newest) })
    } else if metadata.is_file() {
        (metadata.len(), 1, unix_secs(metadata.modified().ok()))
    } else {
        return Measurement {
            finding: None,
            errors,
        };
    };

    if size_bytes < min_size {
        return Measurement {
            finding: None,
            errors,
        };
    }

    Measurement {
        finding: Some(Finding {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            category: rule.category,
            risk: rule.risk,
            recreate: rule.recreate,
            path: path.to_path_buf(),
            size_bytes,
            file_count,
            last_modified,
            description: rule.description.clone(),
        }),
        errors,
    }
}

fn expand_home(pattern: &str) -> String {
    if let Some(rest) = pattern.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().replace('\\', "/");
        }
    }
    pattern.to_string()
}

fn unix_secs(time: Option<SystemTime>) -> Option<u64> {
    time?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Recreate;
    use std::fs;
    use tempfile::tempdir;

    fn test_rule(path: &Path) -> Rule {
        Rule {
            id: "fixture".to_string(),
            name: "Fixture".to_string(),
            description: "A fixture rule".to_string(),
            os: vec![std::env::consts::OS.to_string()],
            category: Category::Developer,
            risk: Risk::Safe,
            recreate: Recreate::Automatic,
            paths: vec![path.display().to_string().replace('\\', "/")],
        }
    }

    #[test]
    fn sums_files_recursively() {
        let dir = tempdir().expect("temporary directory");
        let cache = dir.path().join("cache");
        fs::create_dir_all(cache.join("nested")).expect("nested directory");
        fs::write(cache.join("a.txt"), b"hello").expect("file a");
        fs::write(cache.join("nested/b.txt"), b"world!").expect("file b");

        let report = scan(&ScanOptions {
            os: std::env::consts::OS.to_string(),
            rules: vec![test_rule(&cache)],
            min_size: 0,
            categories: Vec::new(),
            max_risk: None,
        });

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].size_bytes, 11);
        assert_eq!(report.findings[0].file_count, 2);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn honors_min_size() {
        let dir = tempdir().expect("temporary directory");
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).expect("cache directory");
        fs::write(cache.join("small.bin"), b"small").expect("small file");

        let report = scan(&ScanOptions {
            os: std::env::consts::OS.to_string(),
            rules: vec![test_rule(&cache)],
            min_size: 1024,
            categories: Vec::new(),
            max_risk: None,
        });

        assert!(report.findings.is_empty());
    }

    #[test]
    fn does_not_double_count_the_same_path() {
        let dir = tempdir().expect("temporary directory");
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).expect("cache directory");
        fs::write(cache.join("file.bin"), b"1234").expect("file");

        let rule = test_rule(&cache);
        let report = scan(&ScanOptions {
            os: std::env::consts::OS.to_string(),
            rules: vec![rule.clone(), rule],
            min_size: 0,
            categories: Vec::new(),
            max_risk: None,
        });

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.total_bytes, 4);
    }
}
