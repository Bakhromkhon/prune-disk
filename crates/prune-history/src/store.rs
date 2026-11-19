use prune_core::{ByteAccounting, Finding, PathEncoding, ScanCompleteness, ScanReport, ScanRootKind};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const HISTORY_SCHEMA_VERSION: i64 = 1;
const MAX_SNAPSHOTS: i64 = 100;
const MAX_DIFF_ROWS: usize = 100;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryMode {
    /// Create the database if needed; used only by explicit `--remember`.
    CreateOrOpen,
    /// Open an existing database without creating or writing.
    ReadOnly,
    /// Open an existing database for explicit history deletion.
    ReadWrite,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("local application data directory is unavailable")]
    DataDirectoryUnavailable,
    #[error("history database path is not a regular file")]
    InvalidDatabasePath,
    #[error("history database is a symbolic link; refusing to open it")]
    SymlinkDatabase,
    #[error("unsupported history database schema version {0}")]
    UnsupportedDatabaseVersion(i64),
    #[error("history database is missing its private root identity")]
    MissingRootIdentity,
    #[error("history database belongs to a different home scope; saving/diff refused")]
    RootMismatch,
    #[error("only schema v2 scans of the home scope can be saved to history")]
    UnsupportedReport,
    #[error("finding path is not under the trusted home directory")]
    PathOutsideHome,
    #[error("finding paths are not unique in this snapshot")]
    DuplicatePath,
    #[error("history is limited to {MAX_SNAPSHOTS} snapshots; delete one or clear history first")]
    SnapshotLimit,
    #[error("snapshot {0} does not exist in local history")]
    SnapshotNotFound(i64),
    #[error("snapshot {0} is {1}; only complete snapshots can be diffed")]
    IncompleteSnapshot(i64, String),
    #[error("snapshots have different scan scopes; refusing to compare them")]
    ScopeMismatch,
    #[error("history value is malformed: {0}")]
    Corrupt(String),
    #[error("history access mode does not allow this operation")]
    ReadOnly,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct SnapshotSummary {
    pub id: i64,
    pub scanned_at: u64,
    pub platform: String,
    pub completeness: ScanCompleteness,
    pub total_bytes: u64,
    pub total_files: u64,
    pub finding_count: u64,
    pub diagnostic_count: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    New,
    NoLongerObserved,
    Changed,
}

impl ChangeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::New => "Новое в отчёте",
            Self::NoLongerObserved => "Больше не наблюдается",
            Self::Changed => "Размер изменился",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub kind: ChangeKind,
    /// Home-relative display path; never an authorization token.
    pub path: String,
    pub path_encoding: PathEncoding,
    pub rule_name: String,
    pub before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub byte_delta: i128,
    pub before_files: Option<u64>,
    pub after_files: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub older: SnapshotSummary,
    pub newer: SnapshotSummary,
    pub added: u64,
    pub no_longer_observed: u64,
    pub changed: u64,
    /// Difference of logical report totals, not reclaimed storage.
    pub logical_byte_delta: i128,
    pub changes: Vec<Change>,
    pub omitted_changes: u64,
}

pub struct HistoryStore {
    connection: Connection,
    home: PathBuf,
    current_root_identity: String,
    mode: HistoryMode,
}

impl HistoryStore {
    /// Open an existing database read-only, or create it only when explicitly
    /// requested by `CreateOrOpen`. A missing database in other modes returns
    /// `Ok(None)` and does not create files.
    pub fn open(
        path: impl AsRef<Path>,
        home: impl AsRef<Path>,
        mode: HistoryMode,
    ) -> Result<Option<Self>, HistoryError> {
        let path = path.as_ref();
        if mode != HistoryMode::CreateOrOpen && !path.exists() {
            return Ok(None);
        }
        if !path.is_absolute() {
            return Err(HistoryError::InvalidDatabasePath);
        }

        let home = fs::canonicalize(home.as_ref())?;
        if !home.is_dir() || home.parent().is_none() {
            return Err(HistoryError::InvalidDatabasePath);
        }
        let current_root_identity = root_identity(&home)?;

        if mode == HistoryMode::CreateOrOpen {
            prepare_private_directory(path)?;
        }
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(HistoryError::SymlinkDatabase);
        }

        let flags = match mode {
            HistoryMode::CreateOrOpen => {
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
            }
            HistoryMode::ReadOnly => {
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
            }
            HistoryMode::ReadWrite => {
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
            }
        };
        let mut connection = Connection::open_with_flags(path, flags)?;
        connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", true)?;

        if mode != HistoryMode::ReadOnly {
            // The database is local-only. WAL plus FULL synchronous mode makes
            // committed snapshots durable without a transaction over scanning.
            let _: String =
                connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
            connection.pragma_update(None, "synchronous", "FULL")?;
        }

        initialize_schema(&mut connection, &current_root_identity, mode)?;
        if mode != HistoryMode::ReadOnly {
            set_private_database_permissions(path)?;
        }

        Ok(Some(Self {
            connection,
            home,
            current_root_identity,
            mode,
        }))
    }

    /// The platform's local application-data location. Merely asking for this
    /// path does not create it.
    pub fn default_database_path() -> Result<PathBuf, HistoryError> {
        let root = dirs::data_local_dir().ok_or(HistoryError::DataDirectoryUnavailable)?;
        Ok(root.join("Prune").join("history.sqlite3"))
    }

    pub fn open_default(mode: HistoryMode) -> Result<Option<Self>, HistoryError> {
        let path = default_history_path()?;
        let home = dirs::home_dir().ok_or(HistoryError::DataDirectoryUnavailable)?;
        Self::open(path, home, mode)
    }

    /// Persist one report only after an explicit caller action (`--remember`).
    /// Incomplete reports can be retained for diagnostics, but never diffed.
    pub fn save_report(&mut self, report: &ScanReport) -> Result<i64, HistoryError> {
        self.require_writable()?;
        let scope = report
            .scope
            .as_ref()
            .ok_or(HistoryError::UnsupportedReport)?;
        if report.schema_version != 2
            || scope.root_kind != ScanRootKind::Home
            || scope.comparable
            || report.partial
                != matches!(
                    report.completeness,
                    ScanCompleteness::Partial | ScanCompleteness::Cancelled
                )
            || report.cancelled != (report.completeness == ScanCompleteness::Cancelled)
        {
            return Err(HistoryError::UnsupportedReport);
        }

        let root_identity = stored_root_identity(&self.connection)?;
        if root_identity != self.current_root_identity {
            return Err(HistoryError::RootMismatch);
        }

        // Validate exact totals before writing so the local history cannot make
        // a malformed report look like a trustworthy snapshot.
        let mut total_bytes = 0u64;
        let mut total_files = 0u64;
        let mut paths = BTreeSet::new();
        for finding in &report.findings {
            total_bytes = total_bytes
                .checked_add(finding.size_bytes)
                .ok_or_else(|| HistoryError::Corrupt("byte total overflow".into()))?;
            total_files = total_files
                .checked_add(finding.file_count)
                .ok_or_else(|| HistoryError::Corrupt("file total overflow".into()))?;
            let (key, _, _) = relative_finding_path(finding, &self.home)?;
            if !paths.insert(key) {
                return Err(HistoryError::DuplicatePath);
            }
        }
        if total_bytes != report.total_bytes || total_files != report.total_files {
            return Err(HistoryError::Corrupt(
                "report totals do not equal finding totals".into(),
            ));
        }

        let categories = serde_json::to_string(
            &scope
                .categories
                .iter()
                .map(|category| category.as_str())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| HistoryError::Corrupt(error.to_string()))?;
        let max_risk = scope.max_risk.map(|risk| risk.as_str());
        let completeness = completeness_str(report.completeness);
        let byte_accounting = match report.byte_accounting {
            ByteAccounting::Logical => "logical",
        };
        let diagnostic_count = (report.errors.len() as u128)
            .checked_add(u128::from(report.suppressed_errors))
            .ok_or_else(|| HistoryError::Corrupt("diagnostic count overflow".into()))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: i64 =
            transaction.query_row("SELECT COUNT(*) FROM scans", [], |row| row.get(0))?;
        if existing >= MAX_SNAPSHOTS {
            return Err(HistoryError::SnapshotLimit);
        }
        let scan_id: i64 = transaction.query_row(
            "INSERT INTO scans (
                scanned_at, platform, root_kind, rule_set_digest, categories_json,
                max_risk, min_size_bytes, byte_accounting, completeness,
                total_bytes, total_files, finding_count, diagnostic_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             RETURNING id",
            params![
                report.scanned_at.to_string(),
                report.platform,
                "home",
                scope.rule_set_digest,
                categories,
                max_risk,
                scope.min_size_bytes.to_string(),
                byte_accounting,
                completeness,
                report.total_bytes.to_string(),
                report.total_files.to_string(),
                report.findings.len() as i64,
                diagnostic_count.to_string(),
            ],
            |row| row.get(0),
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO findings (
                    scan_id, path_key, path_display, path_encoding, rule_id,
                    rule_name, category, risk, size_bytes, file_count, last_modified, partial
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for finding in &report.findings {
                let (path_key, path_display, path_encoding) =
                    relative_finding_path(finding, &self.home)?;
                insert.execute(params![
                    scan_id,
                    path_key,
                    path_display,
                    path_encoding,
                    finding.rule_id,
                    finding.rule_name,
                    finding.category.as_str(),
                    finding.risk.as_str(),
                    finding.size_bytes.to_string(),
                    finding.file_count.to_string(),
                    finding.last_modified.map(|value| value.to_string()),
                    finding.partial,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(scan_id)
    }

    pub fn list(&self) -> Result<Vec<SnapshotSummary>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, scanned_at, platform, completeness, total_bytes, total_files,
                    finding_count, diagnostic_count
             FROM scans ORDER BY id DESC",
        )?;
        let rows = statement.query_map([], snapshot_from_row)?;
        rows.map(|row| row.map_err(HistoryError::from)).collect()
    }

    pub fn diff(&self, older_id: i64, newer_id: i64) -> Result<Diff, HistoryError> {
        if older_id == newer_id {
            return Err(HistoryError::ScopeMismatch);
        }
        let stored = stored_root_identity(&self.connection)?;
        if stored != self.current_root_identity {
            return Err(HistoryError::RootMismatch);
        }
        let older = load_snapshot(&self.connection, older_id)?;
        let newer = load_snapshot(&self.connection, newer_id)?;
        for snapshot in [&older, &newer] {
            if snapshot.summary.completeness != ScanCompleteness::Complete {
                return Err(HistoryError::IncompleteSnapshot(
                    snapshot.summary.id,
                    completeness_str(snapshot.summary.completeness).to_owned(),
                ));
            }
        }
        if !same_scope(&older, &newer) {
            return Err(HistoryError::ScopeMismatch);
        }

        let older_rows = load_findings(&self.connection, older_id)?;
        let newer_rows = load_findings(&self.connection, newer_id)?;
        let all_keys: BTreeSet<Vec<u8>> = older_rows
            .keys()
            .chain(newer_rows.keys())
            .cloned()
            .collect();
        let mut changes = Vec::new();
        let mut added = 0u64;
        let mut no_longer_observed = 0u64;
        let mut changed = 0u64;

        for key in all_keys {
            match (older_rows.get(&key), newer_rows.get(&key)) {
                (None, Some(after)) => {
                    added += 1;
                    changes.push(Change {
                        kind: ChangeKind::New,
                        path: after.path_display.clone(),
                        path_encoding: after.path_encoding,
                        rule_name: after.rule_name.clone(),
                        before_bytes: None,
                        after_bytes: Some(after.size_bytes),
                        byte_delta: i128::from(after.size_bytes),
                        before_files: None,
                        after_files: Some(after.file_count),
                    });
                }
                (Some(before), None) => {
                    no_longer_observed += 1;
                    changes.push(Change {
                        kind: ChangeKind::NoLongerObserved,
                        path: before.path_display.clone(),
                        path_encoding: before.path_encoding,
                        rule_name: before.rule_name.clone(),
                        before_bytes: Some(before.size_bytes),
                        after_bytes: None,
                        byte_delta: -i128::from(before.size_bytes),
                        before_files: Some(before.file_count),
                        after_files: None,
                    });
                }
                (Some(before), Some(after))
                    if before.size_bytes != after.size_bytes
                        || before.file_count != after.file_count =>
                {
                    changed += 1;
                    changes.push(Change {
                        kind: ChangeKind::Changed,
                        path: after.path_display.clone(),
                        path_encoding: after.path_encoding,
                        rule_name: after.rule_name.clone(),
                        before_bytes: Some(before.size_bytes),
                        after_bytes: Some(after.size_bytes),
                        byte_delta: i128::from(after.size_bytes) - i128::from(before.size_bytes),
                        before_files: Some(before.file_count),
                        after_files: Some(after.file_count),
                    });
                }
                _ => {}
            }
        }

        changes.sort_by(|left, right| {
            right
                .byte_delta
                .unsigned_abs()
                .cmp(&left.byte_delta.unsigned_abs())
                .then_with(|| left.path.cmp(&right.path))
        });
        let omitted_changes = changes.len().saturating_sub(MAX_DIFF_ROWS) as u64;
        changes.truncate(MAX_DIFF_ROWS);

        Ok(Diff {
            logical_byte_delta: i128::from(newer.summary.total_bytes)
                - i128::from(older.summary.total_bytes),
            older: older.summary,
            newer: newer.summary,
            added,
            no_longer_observed,
            changed,
            changes,
            omitted_changes,
        })
    }

    pub fn delete(&mut self, id: i64) -> Result<(), HistoryError> {
        self.require_writable()?;
        let changed = self
            .connection
            .execute("DELETE FROM scans WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(HistoryError::SnapshotNotFound(id));
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<u64, HistoryError> {
        self.require_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = transaction.execute("DELETE FROM scans", [])? as u64;
        transaction.commit()?;
        Ok(removed)
    }

    pub const fn max_snapshots() -> i64 {
        MAX_SNAPSHOTS
    }

    fn require_writable(&self) -> Result<(), HistoryError> {
        if self.mode == HistoryMode::ReadOnly {
            Err(HistoryError::ReadOnly)
        } else {
            Ok(())
        }
    }
}

pub fn default_history_path() -> Result<PathBuf, HistoryError> {
    let root = dirs::data_local_dir().ok_or(HistoryError::DataDirectoryUnavailable)?;
    Ok(root.join("Prune").join("history.sqlite3"))
}

fn prepare_private_directory(path: &Path) -> Result<(), HistoryError> {
    let parent = path.parent().ok_or(HistoryError::InvalidDatabasePath)?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_database_permissions(path: &Path) -> Result<(), HistoryError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn initialize_schema(
    connection: &mut Connection,
    current_root_identity: &str,
    mode: HistoryMode,
) -> Result<(), HistoryError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        HISTORY_SCHEMA_VERSION => {
            stored_root_identity(connection)?;
        }
        0 if mode == HistoryMode::CreateOrOpen => {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE history_metadata (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                 ) WITHOUT ROWID;
                 CREATE TABLE scans (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    scanned_at TEXT NOT NULL,
                    platform TEXT NOT NULL,
                    root_kind TEXT NOT NULL CHECK (root_kind = 'home'),
                    rule_set_digest TEXT NOT NULL,
                    categories_json TEXT NOT NULL,
                    max_risk TEXT,
                    min_size_bytes TEXT NOT NULL,
                    byte_accounting TEXT NOT NULL CHECK (byte_accounting = 'logical'),
                    completeness TEXT NOT NULL CHECK (completeness IN ('complete','partial','cancelled','unknown')),
                    total_bytes TEXT NOT NULL,
                    total_files TEXT NOT NULL,
                    finding_count INTEGER NOT NULL CHECK (finding_count >= 0),
                    diagnostic_count TEXT NOT NULL
                 );
                 CREATE TABLE findings (
                    scan_id INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
                    path_key BLOB NOT NULL,
                    path_display TEXT NOT NULL,
                    path_encoding TEXT NOT NULL CHECK (path_encoding IN ('utf8','lossy_utf8')),
                    rule_id TEXT NOT NULL,
                    rule_name TEXT NOT NULL,
                    category TEXT NOT NULL,
                    risk TEXT NOT NULL,
                    size_bytes TEXT NOT NULL,
                    file_count TEXT NOT NULL,
                    last_modified TEXT,
                    partial INTEGER NOT NULL CHECK (partial IN (0,1)),
                    PRIMARY KEY (scan_id, path_key)
                 ) WITHOUT ROWID;
                 CREATE INDEX findings_by_scan ON findings(scan_id);",
            )?;
            transaction.execute(
                "INSERT INTO history_metadata (key, value) VALUES ('root_identity', ?1)",
                [current_root_identity],
            )?;
            transaction.pragma_update(None, "user_version", HISTORY_SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        other => return Err(HistoryError::UnsupportedDatabaseVersion(other)),
    }
    Ok(())
}

fn stored_root_identity(connection: &Connection) -> Result<String, HistoryError> {
    connection
        .query_row(
            "SELECT value FROM history_metadata WHERE key = 'root_identity'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(HistoryError::MissingRootIdentity)
}

fn root_identity(home: &Path) -> Result<String, HistoryError> {
    let home = fs::canonicalize(home)?;
    let metadata = fs::metadata(&home)?;
    let mut hasher = Sha256::new();
    hasher.update(b"prune-history-root-v1\0");
    hasher.update(std::env::consts::OS.as_bytes());
    hasher.update([0]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        hasher.update(home.to_string_lossy().to_lowercase().as_bytes());
        hasher.update(metadata.creation_time().to_le_bytes());
    }
    #[cfg(not(any(unix, windows)))]
    {
        hasher.update(home.to_string_lossy().as_bytes());
    }

    let digest = hasher.finalize();
    let mut result = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    Ok(result)
}

fn relative_finding_path(
    finding: &Finding,
    home: &Path,
) -> Result<(Vec<u8>, String, &'static str), HistoryError> {
    let relative = finding
        .path
        .strip_prefix(home)
        .map_err(|_| HistoryError::PathOutsideHome)?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(HistoryError::PathOutsideHome);
    }
    let key = path_key(relative);
    let display = relative.to_string_lossy().into_owned();
    let encoding = if relative.to_str().is_some() {
        "utf8"
    } else {
        "lossy_utf8"
    };
    Ok((key, display, encoding))
}

#[cfg(unix)]
fn path_key(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_key(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn path_key(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn completeness_str(value: ScanCompleteness) -> &'static str {
    match value {
        ScanCompleteness::Complete => "complete",
        ScanCompleteness::Partial => "partial",
        ScanCompleteness::Cancelled => "cancelled",
        ScanCompleteness::Unknown => "unknown",
    }
}

fn parse_u64(value: String, field: &str) -> Result<u64, HistoryError> {
    value
        .parse()
        .map_err(|_| HistoryError::Corrupt(format!("invalid {field}")))
}

fn snapshot_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotSummary> {
    let id: i64 = row.get(0)?;
    let scanned_at: String = row.get(1)?;
    let platform: String = row.get(2)?;
    let completeness: String = row.get(3)?;
    let total_bytes: String = row.get(4)?;
    let total_files: String = row.get(5)?;
    let finding_count: i64 = row.get(6)?;
    let diagnostic_count: String = row.get(7)?;
    let corrupt = |field: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(io::Error::new(io::ErrorKind::InvalidData, field.to_owned())),
        )
    };
    let scanned_at = scanned_at.parse().map_err(|_| corrupt("scanned_at"))?;
    let total_bytes = total_bytes.parse().map_err(|_| corrupt("total_bytes"))?;
    let total_files = total_files.parse().map_err(|_| corrupt("total_files"))?;
    let diagnostic_count = diagnostic_count
        .parse()
        .map_err(|_| corrupt("diagnostic_count"))?;
    let completeness = match completeness.as_str() {
        "complete" => ScanCompleteness::Complete,
        "partial" => ScanCompleteness::Partial,
        "cancelled" => ScanCompleteness::Cancelled,
        "unknown" => ScanCompleteness::Unknown,
        _ => return Err(corrupt("completeness")),
    };
    Ok(SnapshotSummary {
        id,
        scanned_at,
        platform,
        completeness,
        total_bytes,
        total_files,
        finding_count: finding_count as u64,
        diagnostic_count,
    })
}

struct StoredSnapshot {
    summary: SnapshotSummary,
    root_kind: String,
    rule_set_digest: String,
    categories_json: String,
    max_risk: Option<String>,
    min_size_bytes: String,
    byte_accounting: String,
}

fn load_snapshot(connection: &Connection, id: i64) -> Result<StoredSnapshot, HistoryError> {
    let row = connection
        .query_row(
            "SELECT id, scanned_at, platform, completeness, total_bytes, total_files,
                    finding_count, diagnostic_count, root_kind, rule_set_digest,
                    categories_json, max_risk, min_size_bytes, byte_accounting
             FROM scans WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    snapshot_from_row(row)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                ))
            },
        )
        .optional()?
        .ok_or(HistoryError::SnapshotNotFound(id))?;
    Ok(StoredSnapshot {
        summary: row.0,
        root_kind: row.1,
        rule_set_digest: row.2,
        categories_json: row.3,
        max_risk: row.4,
        min_size_bytes: row.5,
        byte_accounting: row.6,
    })
}

fn same_scope(left: &StoredSnapshot, right: &StoredSnapshot) -> bool {
    left.root_kind == right.root_kind
        && left.rule_set_digest == right.rule_set_digest
        && left.categories_json == right.categories_json
        && left.max_risk == right.max_risk
        && left.min_size_bytes == right.min_size_bytes
        && left.byte_accounting == right.byte_accounting
        && left.summary.platform == right.summary.platform
}

#[derive(Debug, Clone)]
struct StoredFinding {
    path_display: String,
    path_encoding: PathEncoding,
    rule_name: String,
    size_bytes: u64,
    file_count: u64,
}

fn load_findings(
    connection: &Connection,
    scan_id: i64,
) -> Result<BTreeMap<Vec<u8>, StoredFinding>, HistoryError> {
    let mut statement = connection.prepare(
        "SELECT path_key, path_display, path_encoding, rule_name, size_bytes, file_count
         FROM findings WHERE scan_id = ?1",
    )?;
    let rows = statement.query_map([scan_id], |row| {
        let size: String = row.get(4)?;
        let count: String = row.get(5)?;
        Ok((
            row.get::<_, Vec<u8>>(0)?,
            StoredFinding {
                path_display: row.get(1)?,
                path_encoding: match row.get::<_, String>(2)?.as_str() {
                    "utf8" => PathEncoding::Utf8,
                    "lossy_utf8" => PathEncoding::LossyUtf8,
                    _ => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(io::Error::new(io::ErrorKind::InvalidData, "path_encoding")),
                        ));
                    }
                },
                rule_name: row.get(3)?,
                size_bytes: parse_u64(size, "size_bytes").map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            error.to_string(),
                        )),
                    )
                })?,
                file_count: parse_u64(count, "file_count").map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            error.to_string(),
                        )),
                    )
                })?,
            },
        ))
    })?;
    let mut findings = BTreeMap::new();
    for row in rows {
        let (key, finding) = row?;
        if findings.insert(key, finding).is_some() {
            return Err(HistoryError::Corrupt("duplicate finding path key".into()));
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prune_core::{Category, Recreate, Risk, Rule, ScanOptions, ValidatedRuleSet, scan};
    use std::fs;
    use tempfile::tempdir;

    fn fixture_report(home: &Path, file: &Path) -> ScanReport {
        let rule = Rule {
            id: "fixture".into(),
            name: "Fixture cache".into(),
            description: "Temporary history fixture".into(),
            os: vec![std::env::consts::OS.into()],
            category: Category::Developer,
            risk: Risk::Safe,
            recreate: Recreate::Automatic,
            paths: vec!["~/cache/*".into()],
        };
        let options = ScanOptions {
            rules: ValidatedRuleSet::for_trusted_root(vec![rule], home).unwrap(),
            ..ScanOptions::default()
        };
        let mut report = scan(&options);
        report.scope.as_mut().unwrap().root_kind = ScanRootKind::Home;
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, fs::canonicalize(file).unwrap());
        report
    }

    fn save_fixture(store: &mut HistoryStore, home: &Path, file: &Path, contents: &[u8]) -> i64 {
        fs::write(file, contents).unwrap();
        let report = fixture_report(home, file);
        store.save_report(&report).unwrap()
    }

    fn open_store(temp: &tempfile::TempDir, home: &Path) -> HistoryStore {
        HistoryStore::open(
            temp.path().join("history.sqlite3"),
            home,
            HistoryMode::CreateOrOpen,
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn saves_only_metadata_relative_paths_and_reads_back_a_snapshot() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("cache")).unwrap();
        let file = home.join("cache/item.bin");
        let mut store = open_store(&temp, &home);
        let id = save_fixture(&mut store, &home, &file, b"hello");
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].total_bytes, 5);
        assert_eq!(list[0].completeness, ScanCompleteness::Complete);

        let root = root_identity(&home).unwrap();
        assert!(!root.contains(&home.to_string_lossy().to_string()));
        let stored = stored_root_identity(&store.connection).unwrap();
        assert_eq!(stored, root);

        let path: String = store
            .connection
            .query_row("SELECT path_display FROM findings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(path, "cache/item.bin");
        assert!(!path.contains(&home.to_string_lossy().to_string()));
    }

    #[test]
    fn diff_reports_observations_not_deletion_and_checks_exact_scope() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("cache")).unwrap();
        let file = home.join("cache/item.bin");
        let mut store = open_store(&temp, &home);
        let older = save_fixture(&mut store, &home, &file, b"12345");
        let newer = save_fixture(&mut store, &home, &file, b"1234567890");
        let diff = store.diff(older, newer).unwrap();
        assert_eq!(diff.changed, 1);
        assert_eq!(diff.added, 0);
        assert_eq!(diff.no_longer_observed, 0);
        assert_eq!(diff.logical_byte_delta, 5);
        assert_eq!(diff.changes[0].byte_delta, 5);
        assert_eq!(diff.changes[0].kind, ChangeKind::Changed);
        assert!(!diff.changes[0].path.starts_with('/'));
        assert!(
            !diff.changes[0]
                .path
                .to_lowercase()
                .contains(&home.to_string_lossy().to_lowercase())
        );

        let changed_scope = {
            let mut report = fixture_report(&home, &file);
            report.scope.as_mut().unwrap().min_size_bytes = 1;
            report
        };
        let other = store.save_report(&changed_scope).unwrap();
        assert!(matches!(
            store.diff(older, other),
            Err(HistoryError::ScopeMismatch)
        ));
    }

    #[test]
    fn partial_unknown_or_cancelled_scans_cannot_prove_disappearance() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("cache")).unwrap();
        let file = home.join("cache/item.bin");
        let mut store = open_store(&temp, &home);
        let complete = save_fixture(&mut store, &home, &file, b"12345");

        let mut partial = fixture_report(&home, &file);
        partial.completeness = ScanCompleteness::Partial;
        partial.partial = true;
        partial.errors.push(prune_core::ScanError {
            path: file.display().to_string(),
            message: "permission denied".into(),
        });
        let partial_id = store.save_report(&partial).unwrap();
        assert!(matches!(
            store.diff(complete, partial_id),
            Err(HistoryError::IncompleteSnapshot(_, _))
        ));

        let mut unknown = fixture_report(&home, &file);
        unknown.completeness = ScanCompleteness::Unknown;
        unknown.partial = false;
        let unknown_id = store.save_report(&unknown).unwrap();
        assert!(matches!(
            store.diff(complete, unknown_id),
            Err(HistoryError::IncompleteSnapshot(_, _))
        ));
    }

    #[test]
    fn empty_untrusted_root_is_not_saved_and_root_mismatch_blocks_saves() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let other_home = temp.path().join("other-home");
        fs::create_dir_all(home.join("cache")).unwrap();
        fs::create_dir_all(other_home.join("cache")).unwrap();
        let file = home.join("cache/item.bin");
        let other_file = other_home.join("cache/item.bin");
        fs::write(&file, b"original").unwrap();
        fs::write(&other_file, b"different").unwrap();
        let mut store = open_store(&temp, &home);
        let report = fixture_report(&home, &file);
        store.save_report(&report).unwrap();

        let mut other = fixture_report(&other_home, &other_file);
        assert!(matches!(
            store.save_report(&other),
            Err(HistoryError::PathOutsideHome)
        ));
        other.scope.as_mut().unwrap().root_kind = ScanRootKind::Custom;
        assert!(matches!(
            store.save_report(&other),
            Err(HistoryError::UnsupportedReport)
        ));
    }

    #[test]
    fn clear_and_delete_are_explicit_and_dont_delete_user_files() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join("cache")).unwrap();
        let file = home.join("cache/item.bin");
        let mut store = open_store(&temp, &home);
        let id = save_fixture(&mut store, &home, &file, b"1234");
        store.delete(id).unwrap();
        assert!(store.list().unwrap().is_empty());

        save_fixture(&mut store, &home, &file, b"12345");
        assert_eq!(store.clear().unwrap(), 1);
        assert!(store.list().unwrap().is_empty());
        assert_eq!(fs::read(&file).unwrap(), b"12345");
    }

    #[cfg(unix)]
    #[test]
    fn relative_path_keys_preserve_non_utf8_names() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"cache/name-\xff".to_vec()));
        assert_eq!(path_key(&path), b"cache/name-\xff");
    }
}
