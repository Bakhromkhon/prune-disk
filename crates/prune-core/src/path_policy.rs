//! One lexical grammar and conservative filesystem link policy.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct ValidatedPath {
    pub(crate) components: Vec<String>,
    pub(crate) children: bool,
}

pub(crate) fn parse_path_pattern(pattern: &str, root: &Path) -> Result<ValidatedPath, String> {
    const BROAD_ROOTS: &[&str] = &[
        "Library",
        "Documents",
        "Desktop",
        "Downloads",
        "Pictures",
        "Movies",
        "Music",
        "Applications",
        "Projects",
        "Developer",
        "Public",
    ];
    if pattern.is_empty()
        || pattern.trim() != pattern
        || pattern
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | '$'))
    {
        return Err("empty, padded, control, backslash or variable path".into());
    }
    let root = root
        .to_str()
        .ok_or("root is not Unicode")?
        .replace('\\', "/");
    let prefix = format!("{root}/");
    let rest = pattern
        .strip_prefix("~/")
        .or_else(|| pattern.strip_prefix(&prefix))
        .ok_or("path must start with ~/ or be under the trusted root")?;
    let components: Vec<&str> = rest.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        if *component == "*" && index + 1 == components.len() {
            continue;
        }
        if component.is_empty()
            || matches!(*component, "." | "..")
            || component.trim() != *component
            || component.ends_with('.')
            || component.chars().any(|c| {
                matches!(
                    c,
                    '*' | '?' | '[' | ']' | '{' | '}' | ':' | '~' | '<' | '>' | '|' | '"'
                )
            })
        {
            return Err("only literal components and a terminal /* are supported".into());
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("Windows device component".into());
        }
    }
    if components.len() < 2 {
        return Err("path is too broad; use a more specific path".into());
    }
    if BROAD_ROOTS
        .iter()
        .any(|root| root.eq_ignore_ascii_case(components[0]))
        && components.len() == 2
        && components[1] == "*"
    {
        return Err("path is too broad; use a more specific path".into());
    }
    let children = components.last() == Some(&"*");
    Ok(ValidatedPath {
        components: components
            .into_iter()
            .filter(|c| *c != "*")
            .map(str::to_owned)
            .collect(),
        children,
    })
}

pub(crate) fn forbidden_metadata(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reject every reparse-point type, including junctions and unknown tags.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

/// Check all absolute ancestors. This is best-effort, NOT a TOCTOU-proof sandbox.
pub(crate) fn checked_metadata(path: &Path) -> io::Result<fs::Metadata> {
    let ancestors: Vec<_> = path.ancestors().collect();
    let mut result = None;
    for ancestor in ancestors.into_iter().rev() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if forbidden_metadata(&metadata) {
            return Err(io::Error::other("symlink or reparse point rejected"));
        }
        if ancestor != path && !metadata.is_dir() {
            return Err(io::Error::other("ancestor is not a directory"));
        }
        result = Some(metadata);
    }
    result.ok_or_else(|| io::Error::other("empty path"))
}

/// Normalize an existing candidate without trusting canonicalize to reject links.
/// Both spellings are checked, and the canonical path must stay inside the
/// already canonical trusted root. These checks still have TOCTOU windows.
pub(crate) fn canonical_candidate(path: &Path, root: &Path) -> io::Result<PathBuf> {
    checked_metadata(path)?;
    let canonical = fs::canonicalize(path)?;
    if canonical == root || !canonical.starts_with(root) {
        return Err(io::Error::other(
            "canonical candidate escapes the trusted root",
        ));
    }
    checked_metadata(path)?;
    checked_metadata(&canonical)?;
    Ok(canonical)
}

/// Obtain an identity only for regular files/directories, never special files
/// such as FIFOs. Handles stay alive while compared (required on Windows).
#[cfg(any(test, not(unix)))]
pub(crate) fn checked_handle(path: &Path) -> io::Result<same_file::Handle> {
    let metadata = checked_metadata(path)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(io::Error::other("special file rejected"));
    }
    let identity = same_file::Handle::from_path(path)?;
    checked_metadata(path)?;
    Ok(identity)
}

#[cfg(not(unix))]
pub(crate) use checked_handle as checked_identity;

/// Closed stat identity: no file descriptors are retained. Type participates in
/// equality so replacement with another kind cannot be mistaken for an alias.
/// Inode reuse and changes between checks remain possible (not a sandbox).
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Identity {
    device: u64,
    inode: u64,
    directory: bool,
}

#[cfg(unix)]
pub(crate) fn checked_identity(path: &Path) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = checked_metadata(path)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(io::Error::other("special file rejected"));
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
        directory: metadata.is_dir(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_candidate_rejects_an_existing_path_outside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(root.path()).unwrap();
        let outside = fs::canonicalize(outside.path()).unwrap();
        assert!(canonical_candidate(&outside, &root).is_err());
        assert!(canonical_candidate(&root, &root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_candidate_checks_links_before_following_them() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/cache")).unwrap();
        fs::write(dir.path().join("app/cache/local"), b"fixture").unwrap();
        symlink(dir.path().join("app/cache"), dir.path().join("alias")).unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        assert!(canonical_candidate(&root.join("alias/local"), &root).is_err());
    }
}
