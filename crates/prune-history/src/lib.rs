#![forbid(unsafe_code)]
#![doc = "Opt-in local scan metadata history and read-only diffs."]

mod store;

pub use store::{
    Change, ChangeKind, Diff, HistoryError, HistoryMode, HistoryStore, SnapshotSummary,
    default_history_path,
};
