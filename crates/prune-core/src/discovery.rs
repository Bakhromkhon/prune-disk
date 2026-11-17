//! Controlled immediate-child expansion and conservative overlap resolution.
use crate::control::{ErrorLog, Progress};
use crate::model::{Recreate, Rule};
use crate::path_policy::{
    canonical_candidate, checked_identity, checked_metadata, forbidden_metadata,
};
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
                                                    if !push_candidate(
                                                        &mut candidates,
                                                        entry.rule.clone(),
                                                        path,
                                                        errors,
                                                        progress,
                                                    ) {
                                                        return Vec::new();
                                                    }
                                                }
                                                Err(error) => {
                                                    normalization_failed = true;
                                                    record_error(errors, &child.path(), error);
                                                }
                                            },
                                            Err(error) => {
                                                // A known link is intentionally excluded by
                                                // measurement too. Other failures may hide a
                                                // high-risk child: discard the whole pass.
                                                if !fs::symlink_metadata(child.path())
                                                    .is_ok_and(|m| forbidden_metadata(&m))
                                                {
                                                    normalization_failed = true;
                                                }
                                                record_error(errors, &child.path(), error)
                                            }
                                        },
                                        Err(error) => {
                                            normalization_failed = true;
                                            record_error(errors, &path, error);
                                        }
                                    }
                                    progress.work(1, 0, 0, 0);
                                }
                            }
                            Err(error) => {
                                normalization_failed = true;
                                record_error(errors, &path, error);
                            }
                        }
                    }
                    Ok(_) if !pattern.children => match canonical_candidate(&path, root) {
                        Ok(path) => {
                            if !push_candidate(
                                &mut candidates,
                                entry.rule.clone(),
                                path,
                                errors,
                                progress,
                            ) {
                                return Vec::new();
                            }
                        }
                        Err(error) => {
                            normalization_failed = true;
                            record_error(errors, &path, error);
                        }
                    },
                    Ok(_) => {
                        normalization_failed = true;
                        record_error(errors, &path, "wildcard parent is not a directory");
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        normalization_failed = true;
                        record_error(errors, &path, error);
                    }
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
    match reconcile(candidates, root, root_identity, progress) {
        Ok(disjoint) => disjoint,
        Err(error) => {
            record_error(errors, root, error);
            Vec::new()
        }
    }
}

fn push_candidate(
    candidates: &mut Vec<(Rule, PathBuf)>,
    rule: Rule,
    path: PathBuf,
    errors: &mut ErrorLog,
    progress: &Progress<'_>,
) -> bool {
    if candidates.len() >= progress.control.candidate_limit() {
        record_error(
            errors,
            &path,
            "candidate limit reached; discovery discarded",
        );
        return false;
    }
    candidates.push((rule, path));
    true
}

fn merge(parent: &mut Rule, child: &Rule) {
    parent.risk = parent.risk.max(child.risk);
    if parent.recreate != child.recreate {
        parent.recreate = Recreate::Unknown;
    }
}

fn cancelled(progress: &Progress<'_>) -> io::Result<()> {
    if progress.control.is_cancelled() {
        Err(io::Error::other("scan cancelled"))
    } else {
        Ok(())
    }
}

/// Expected O(candidates * depth) identity lookup, with no retained descriptors.
/// All observed ancestors (including merged descendants) are checked again
/// before returning. This detects observed replacements, not all TOCTOU races
/// or inode reuse. Memory also depends on path depth/length, not just count.
#[cfg(unix)]
fn reconcile(
    candidates: Vec<(Rule, PathBuf)>,
    root: &std::path::Path,
    root_identity: crate::path_policy::Identity,
    progress: &Progress<'_>,
) -> io::Result<Vec<(Rule, PathBuf)>> {
    reconcile_with_identity(candidates, root, root_identity, progress, checked_identity)
}

#[cfg(unix)]
fn reconcile_with_identity(
    candidates: Vec<(Rule, PathBuf)>,
    root: &std::path::Path,
    root_identity: crate::path_policy::Identity,
    progress: &Progress<'_>,
    mut identity_of: impl FnMut(&std::path::Path) -> io::Result<crate::path_policy::Identity>,
) -> io::Result<Vec<(Rule, PathBuf)>> {
    use std::collections::HashMap;
    let mut observed = HashMap::from([(root.to_path_buf(), root_identity)]);
    let mut index: HashMap<crate::path_policy::Identity, usize> = HashMap::new();
    let mut disjoint: Vec<(Rule, PathBuf)> = Vec::new();
    for (rule, path) in candidates {
        cancelled(progress)?;
        let mut own = None;
        let mut parent = None;
        let mut reached_root = false;
        for ancestor in path.ancestors().take_while(|p| p.starts_with(root)) {
            cancelled(progress)?;
            let identity = identity_of(ancestor)?;
            if observed
                .insert(ancestor.to_path_buf(), identity)
                .is_some_and(|previous| previous != identity)
            {
                return Err(io::Error::other(
                    "filesystem identity changed; discovery discarded",
                ));
            }
            own.get_or_insert(identity);
            if let Some(&position) = index.get(&identity) {
                let (_, stored_path) = &disjoint[position];
                if identity_of(stored_path)? != identity {
                    return Err(io::Error::other(
                        "candidate identity changed; discovery discarded",
                    ));
                }
                if parent.is_some_and(|previous| previous != position) {
                    return Err(io::Error::other(
                        "ambiguous candidate ancestry; discovery discarded",
                    ));
                }
                parent = Some(position);
            }
            if ancestor == root {
                reached_root = true;
                break;
            }
        }
        if !reached_root {
            return Err(io::Error::other("candidate has no trusted root ancestor"));
        }
        if let Some(position) = parent {
            merge(&mut disjoint[position].0, &rule);
        } else {
            index.insert(own.expect("candidate identity"), disjoint.len());
            disjoint.push((rule, path));
        }
    }
    for (path, identity) in observed {
        cancelled(progress)?;
        if identity_of(&path)? != identity {
            return Err(io::Error::other(
                "filesystem identity changed; discovery discarded",
            ));
        }
    }
    cancelled(progress)?;
    Ok(disjoint)
}

#[cfg(not(unix))]
use reconcile_handles as reconcile;

/// Windows has no stable std API for a closed, persistent file identity.
/// Reopen both sides for each comparison, keeping them alive simultaneously.
/// At most the root plus two transient handles are retained; worst-case work
/// is O(candidates² * depth). Never use hashed handle digests as identity keys.
/// Metadata observations detect some replacements; neither these observations
/// nor same-file comparisons eliminate TOCTOU / filesystem file-index reuse.
#[cfg(any(test, not(unix)))]
fn reconcile_handles(
    candidates: Vec<(Rule, PathBuf)>,
    root: &std::path::Path,
    root_identity: same_file::Handle,
    progress: &Progress<'_>,
) -> io::Result<Vec<(Rule, PathBuf)>> {
    use crate::path_policy::checked_handle;
    let mut observed = std::collections::HashMap::new();
    let mut disjoint: Vec<(Rule, PathBuf)> = Vec::new();
    let mut resolved = Vec::new();
    // Metadata is only a change detector, never an alias/equality key.
    fn observation(
        path: &std::path::Path,
    ) -> io::Result<(
        bool,
        u64,
        Option<std::time::SystemTime>,
        Option<std::time::SystemTime>,
    )> {
        let m = checked_metadata(path)?;
        Ok((m.is_dir(), m.len(), m.created().ok(), m.modified().ok()))
    }
    for (rule, path) in candidates {
        cancelled(progress)?;
        let mut parent = None;
        let mut reached_root = false;
        for ancestor in path.ancestors().take_while(|p| p.starts_with(root)) {
            cancelled(progress)?;
            let state = observation(ancestor)?;
            if observed
                .insert(ancestor.to_path_buf(), state)
                .is_some_and(|old| old != state)
            {
                return Err(io::Error::other(
                    "filesystem metadata changed; discovery discarded",
                ));
            }
            let identity = checked_handle(ancestor)?;
            if ancestor == root {
                if identity != root_identity {
                    return Err(io::Error::other("trusted root identity changed"));
                }
                reached_root = true;
                break;
            }
            for (position, (_, stored_path)) in disjoint.iter().enumerate() {
                cancelled(progress)?;
                // identity and other are both live throughout equality.
                let other = checked_handle(stored_path)?;
                if observation(stored_path)? != observed[stored_path] {
                    return Err(io::Error::other(
                        "candidate metadata changed; discovery discarded",
                    ));
                }
                if identity == other {
                    if parent.is_some_and(|previous| previous != position) {
                        return Err(io::Error::other(
                            "ambiguous candidate ancestry; discovery discarded",
                        ));
                    }
                    parent = Some(position);
                    break;
                }
            }
        }
        if !reached_root {
            return Err(io::Error::other("candidate has no trusted root ancestor"));
        }
        if let Some(position) = parent {
            merge(&mut disjoint[position].0, &rule);
            resolved.push((path, position));
        } else {
            resolved.push((path.clone(), disjoint.len()));
            disjoint.push((rule, path));
        }
    }
    // Do not trust a previously observed alias after its handles were closed.
    // Revalidate every resolved relationship with fresh, simultaneously live
    // handles, including the candidates which were merged away. Abort if a
    // formerly disjoint path now aliases a different output.
    for (path, expected) in resolved {
        let mut matched = false;
        for ancestor in path
            .ancestors()
            .take_while(|p| *p != root && p.starts_with(root))
        {
            cancelled(progress)?;
            let identity = checked_handle(ancestor)?;
            for (position, (_, stored_path)) in disjoint.iter().enumerate() {
                cancelled(progress)?;
                let other = checked_handle(stored_path)?;
                if identity == other {
                    if position != expected {
                        return Err(io::Error::other(
                            "candidate alias changed; discovery discarded",
                        ));
                    }
                    matched = true;
                }
            }
        }
        if !matched {
            return Err(io::Error::other(
                "candidate identity changed; discovery discarded",
            ));
        }
    }
    for (path, state) in observed {
        cancelled(progress)?;
        if observation(&path)? != state {
            return Err(io::Error::other(
                "filesystem metadata changed; discovery discarded",
            ));
        }
    }
    if checked_handle(root)? != root_identity {
        return Err(io::Error::other("trusted root identity changed"));
    }
    cancelled(progress)?;
    Ok(disjoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> Rule {
        Rule {
            id: "fixture".into(),
            name: "fixture".into(),
            description: "fixture".into(),
            os: vec![std::env::consts::OS.into()],
            category: crate::Category::Developer,
            risk: crate::Risk::Safe,
            recreate: Recreate::Automatic,
            paths: vec!["~/app/cache/*".into()],
        }
    }

    #[cfg(unix)]
    #[test]
    fn identity_failure_cancellation_and_observed_replacement_discard_reconciliation() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/file"), b"x").unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let path = root.join("app/cache/file");
        let root_identity = checked_identity(&root).unwrap();
        for mode in ["failure", "cancel", "replacement", "root replacement"] {
            let control = crate::ScanControl::default();
            let progress = Progress::new(&control, &|_| {});
            let mut visits = 0;
            let result = reconcile_with_identity(
                vec![(rule(), path.clone())],
                &root,
                root_identity,
                &progress,
                |p| {
                    if p == path {
                        visits += 1;
                        if visits == 2 {
                            match mode {
                                "failure" => {
                                    return Err(io::Error::other("forced identity failure"));
                                }
                                "cancel" => control.cancel(),
                                "replacement" => return checked_identity(&root),
                                _ => {}
                            }
                        }
                    }
                    if mode == "root replacement" && p == root {
                        return checked_identity(&path);
                    }
                    checked_identity(p)
                },
            );
            assert!(result.is_err(), "{mode}");
        }
    }

    #[test]
    fn bounded_handle_backend_handles_more_than_old_limit_and_merges_never_descendant() {
        use crate::path_policy::checked_handle;
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mut candidates = Vec::new();
        for i in 0..130 {
            let path = root.join(format!("app/cache/{i:03}"));
            fs::create_dir(&path).unwrap();
            candidates.push((rule(), path));
        }
        let child = root.join("app/cache/000/never");
        fs::write(&child, b"x").unwrap();
        let mut never = rule();
        never.risk = crate::Risk::Never;
        never.recreate = Recreate::Manual;
        candidates.push((never, child));
        let control = crate::ScanControl::default();
        let progress = Progress::new(&control, &|_| {});
        let result =
            reconcile_handles(candidates, &root, checked_handle(&root).unwrap(), &progress)
                .unwrap();
        assert_eq!(result.len(), 130);
        assert_eq!(result[0].0.risk, crate::Risk::Never);
        assert_eq!(result[0].0.recreate, Recreate::Unknown);
    }

    #[cfg(unix)]
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
        let identities: Vec<_> = alias
            .ancestors()
            .take_while(|p| p.starts_with(&root))
            .map(|p| checked_identity(p).unwrap())
            .collect();
        let parent = checked_identity(&root.join("app/cache")).unwrap();
        let equal_file = checked_identity(&root.join("app/cache/local")).unwrap();
        assert!(identities.iter().any(|identity| identity == &parent));
        assert_eq!(identities[0], equal_file);
        let mut never = rule();
        never.risk = crate::Risk::Never;
        let candidates = vec![(rule(), root.join("app/cache")), (never, alias)];
        let control = crate::ScanControl::default();
        let progress = Progress::new(&control, &|_| {});
        let result = reconcile(
            candidates.clone(),
            &root,
            checked_identity(&root).unwrap(),
            &progress,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.risk, crate::Risk::Never);
        let result = reconcile_handles(
            candidates,
            &root,
            crate::path_policy::checked_handle(&root).unwrap(),
            &progress,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.risk, crate::Risk::Never);
    }
}
