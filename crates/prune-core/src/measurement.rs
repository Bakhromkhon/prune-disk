//! Best-effort logical byte measurement; not allocated/reclaimable space.
use crate::control::{ErrorLog, Progress};
use crate::model::{ByteAccounting, Finding, Rule};
use crate::path_policy::checked_metadata;
use crate::scan::record_error;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Measurement {
    pub(crate) finding: Option<Finding>,
    pub(crate) errors: ErrorLog,
}

pub(crate) fn measure(
    rule: &Rule,
    path: &Path,
    min_size: u64,
    progress: &Progress<'_>,
) -> Measurement {
    let mut errors = ErrorLog::default();
    if progress.control.is_cancelled() {
        return Measurement {
            finding: None,
            errors,
        };
    }
    let root_metadata = match checked_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            record_error(&mut errors, path, error);
            return Measurement {
                finding: None,
                errors,
            };
        }
    };
    let mut stack = vec![path.to_path_buf()];
    let mut size_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut newest = None;
    while let Some(current) = stack.pop() {
        if progress.control.is_cancelled() {
            break;
        }
        let metadata = match checked_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) => {
                record_error(&mut errors, &current, error);
                continue;
            }
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() != root_metadata.dev() {
                record_error(&mut errors, &current, "filesystem boundary skipped");
                continue;
            }
        }
        #[cfg(not(unix))]
        let _ = &root_metadata;
        if metadata.is_file() {
            match (
                size_bytes.checked_add(metadata.len()),
                file_count.checked_add(1),
            ) {
                (Some(size), Some(count)) => {
                    size_bytes = size;
                    file_count = count;
                    progress.work(0, 1, 1, metadata.len());
                }
                _ => record_error(&mut errors, &current, "logical byte/file count overflow"),
            }
            newest = newest.max(unix_secs(metadata.modified().ok()));
        } else if metadata.is_dir() {
            progress.work(0, 1, 0, 0);
            if progress.control.is_cancelled() {
                break;
            }
            match fs::read_dir(&current) {
                Ok(entries) => {
                    for entry in entries {
                        if progress.control.is_cancelled() {
                            break;
                        }
                        match entry {
                            Ok(entry) => stack.push(entry.path()),
                            Err(error) => record_error(&mut errors, &current, error),
                        }
                    }
                }
                Err(error) => record_error(&mut errors, &current, error),
            }
        } else {
            record_error(&mut errors, &current, "special file rejected");
        }
    }
    let partial = progress.control.is_cancelled() || !errors.is_empty();
    let finding = (size_bytes >= min_size && (!partial || file_count > 0)).then(|| Finding {
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        category: rule.category,
        risk: rule.risk,
        recreate: rule.recreate,
        path: path.to_path_buf(),
        size_bytes,
        file_count,
        last_modified: newest,
        description: rule.description.clone(),
        byte_accounting: ByteAccounting::Logical,
        partial,
    });
    Measurement { finding, errors }
}

pub(crate) fn unix_secs(time: Option<SystemTime>) -> Option<u64> {
    time?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}
