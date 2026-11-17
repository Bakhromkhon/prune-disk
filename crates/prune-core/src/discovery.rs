//! Controlled immediate-child expansion and conservative overlap resolution.
use crate::control::{ErrorLog, MAX_IDENTITY_HANDLES, Progress};
use crate::model::{Recreate, Rule};
use crate::path_policy::{canonical_candidate, checked_identity, checked_metadata};
use crate::rules::ValidatedRuleSet;
use crate::scan::record_error;
use std::{fs, io, path::PathBuf};

pub(crate) fn discover(
    rules: &ValidatedRuleSet,
    os: &str,
    errors: &mut ErrorLog,
    progress: &Progress<'_>,
) -> Vec<(Rule, PathBuf)> {
    let mut candidates = Vec::new();
    if progress.control.is_cancelled() {
        return candidates;
    }
    let Some(root) = &rules.root else {
        return candidates;
    };
    let root_identity = match checked_identity(root) {
        Ok(identity) => identity,
        Err(error) => {
            record_error(errors, root, error);
            return candidates;
        }
    };
    let mut normalization_failed = false;
    if let Some(root) = &rules.root {
        for entry in &rules.entries {
            if !entry.rule.applies_to(os) {
                continue;
            }
            for pattern in &entry.paths {
                if progress.control.is_cancelled() {
                    return Vec::new();
                }
                let mut path = root.clone();
                for component in &pattern.components {
                    path.push(component);
                }
                match checked_metadata(&path) {
                    Ok(metadata) if pattern.children && metadata.is_dir() => {
                        match fs::read_dir(&path) {
                            Ok(entries) => {
                                for child in entries {
                                    if progress.control.is_cancelled() {
                                        return Vec::new();
                                    }
                                    match child {
                                        Ok(child) => match checked_metadata(&child.path()) {
                                            Ok(_) => match canonical_candidate(&child.path(), root)
                                            {
                                                Ok(path) => {
                                                    candidates.push((entry.rule.clone(), path))
                                                }
                                                Err(error) => {
                                                    normalization_failed = true;
                                                    record_error(errors, &child.path(), error);
                                                }
                                            },
                                            Err(error) => {
                                                record_error(errors, &child.path(), error)
                                            }
                                        },
                                        Err(error) => record_error(errors, &path, error),
                                    }
                                    progress.work(1, 0, 0, 0);
                                }
                            }
                            Err(error) => record_error(errors, &path, error),
                        }
                    }
                    Ok(_) if !pattern.children => match canonical_candidate(&path, root) {
                        Ok(path) => candidates.push((entry.rule.clone(), path)),
                        Err(error) => {
                            normalization_failed = true;
                            record_error(errors, &path, error);
                        }
                    },
                    Ok(_) => record_error(errors, &path, "wildcard parent is not a directory"),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => record_error(errors, &path, error),
                }
                progress.work(1, 0, 0, 0);
            }
        }
    }
    // Identity failures must not drop a possibly dangerous descendant while
    // retaining its safe parent. Fail closed for this discovery pass.
    if normalization_failed {
        return Vec::new();
    }
    if progress.control.is_cancelled() {
        return Vec::new();
    }
    // Resolve BEFORE risk/category filtering so an excluded high/never-risk
    // descendant cannot hide within a safe parent.
    candidates.sort_by(|(a, ap), (b, bp)| {
        ap.components()
            .count()
            .cmp(&bp.components().count())
            .then_with(|| ap.cmp(bp))
            .then_with(|| b.risk.cmp(&a.risk))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut disjoint: Vec<(Rule, PathBuf, same_file::Handle)> = Vec::new();
    for (rule, path) in candidates {
        if progress.control.is_cancelled() {
            return Vec::new();
        }
        // canonicalize is not required to return the filesystem's stored case.
        // Compare real identities for self AND every ancestor, not path strings
        // or lowercased names. Parent-first depth order handles aliases whose
        // lexical sort order would otherwise put the descendant first.
        let identities =
            match ancestor_identities(&path, root, &root_identity, progress, disjoint.len() + 1) {
                Ok(identities) => identities,
                Err(error) => {
                    record_error(errors, &path, error);
                    return Vec::new();
                }
            };
        if let Some((parent_rule, _, _)) = disjoint
            .iter_mut()
            .find(|(_, _, parent)| identities.iter().any(|identity| identity == parent))
        {
            parent_rule.risk = parent_rule.risk.max(rule.risk);
            if parent_rule.recreate != rule.recreate {
                parent_rule.recreate = Recreate::Unknown;
            }
        } else {
            disjoint.push((
                rule,
                path,
                identities.into_iter().next().expect("candidate identity"),
            ));
        }
    }
    disjoint
        .into_iter()
        .map(|(rule, path, _)| (rule, path))
        .collect()
}

fn ancestor_identities(
    path: &std::path::Path,
    root: &std::path::Path,
    root_identity: &same_file::Handle,
    progress: &Progress<'_>,
    retained: usize,
) -> io::Result<Vec<same_file::Handle>> {
    let mut identities = Vec::new();
    for ancestor in path.ancestors() {
        if progress.control.is_cancelled() {
            return Err(io::Error::other("scan cancelled"));
        }
        if !ancestor.starts_with(root) {
            break;
        }
        if retained + identities.len() >= MAX_IDENTITY_HANDLES {
            return Err(io::Error::other(
                "identity handle limit reached; discovery discarded",
            ));
        }
        let identity = checked_identity(ancestor)?;
        if ancestor == root && &identity != root_identity {
            return Err(io::Error::other("trusted root identity changed"));
        }
        identities.push(identity);
        if ancestor == root {
            return Ok(identities);
        }
    }
    Err(io::Error::other("candidate has no trusted root ancestor"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_chain_handles_non_normalized_case_even_if_canonicalize_normalizes_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/local"), b"fixture").unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let alias = root.join("app/CACHE/LOCAL");
        match fs::symlink_metadata(&alias) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!("SKIP identity case-alias regression: filesystem is case-sensitive");
                return;
            }
            Err(error) => panic!("case fixture metadata: {error}"),
            Ok(_) => {}
        }
        // Deliberately bypass canonicalization in this internal helper test:
        // guarantees the fallback is exercised on both kinds of canonicalize.
        let control = crate::ScanControl::default();
        let progress = Progress::new(&control, &|_| {});
        let identities = ancestor_identities(
            &alias,
            &root,
            &checked_identity(&root).unwrap(),
            &progress,
            1,
        )
        .unwrap();
        let parent = checked_identity(&root.join("app/cache")).unwrap();
        let equal_file = checked_identity(&root.join("app/cache/local")).unwrap();
        assert!(identities.iter().any(|identity| identity == &parent));
        assert_eq!(identities[0], equal_file);
    }
}
