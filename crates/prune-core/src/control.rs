//! UI-independent cooperative cancellation and serialized progress snapshots.
use crate::model::ScanError;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub const MAX_SCAN_WORKERS: usize = 16;
pub const MAX_SCAN_ERRORS: usize = 256;
/// Bounds collected discovery candidates, including aliases, before filtering.
/// This is not a bound on traversal entries or path lengths.
pub const MAX_SCAN_CANDIDATES: usize = 10_000;

#[derive(Debug, Clone)]
/// Clones share a one-way cancellation token; worker limits are per value.
pub struct ScanControl {
    cancelled: Arc<AtomicBool>,
    workers: usize,
    candidates: usize,
}

impl Default for ScanControl {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            workers: 2,
            candidates: MAX_SCAN_CANDIDATES,
        }
    }
}

impl ScanControl {
    /// Lower the discovery memory budget (clamped to 1..=MAX_SCAN_CANDIDATES).
    /// Overflow discards the entire discovery pass and reports a partial scan.
    pub fn with_candidate_limit(mut self, candidates: usize) -> Self {
        self.candidates = candidates.clamp(1, MAX_SCAN_CANDIDATES);
        self
    }
    pub fn candidate_limit(&self) -> usize {
        self.candidates
    }
    /// Clamp the per-scan worker limit to 1..=MAX_SCAN_WORKERS.
    pub fn with_worker_count(mut self, workers: usize) -> Self {
        self.workers = workers.clamp(1, MAX_SCAN_WORKERS);
        self
    }
    pub fn worker_count(&self) -> usize {
        self.workers
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStage {
    Discovering,
    Measuring,
    Completed,
    Cancelled,
}

/// Observed work, not a percentage or reclaimable space. File/byte counters
/// include work below min_size and can exceed the filtered report totals.
#[derive(Debug, Clone, Copy)]
pub struct ScanProgress {
    pub stage: ScanStage,
    /// Discovery patterns and wildcard children attempted, before deduplication.
    pub candidates: u64,
    /// Successfully examined regular files and directories during measurement.
    pub entries: u64,
    pub files_measured: u64,
    pub bytes_measured: u64,
}

pub(crate) struct Progress<'a> {
    pub control: &'a ScanControl,
    callback: &'a (dyn Fn(ScanProgress) + Send + Sync),
    state: Mutex<(ScanProgress, Instant, bool)>,
}

impl<'a> Progress<'a> {
    pub fn new(
        control: &'a ScanControl,
        callback: &'a (dyn Fn(ScanProgress) + Send + Sync),
    ) -> Self {
        Self {
            control,
            callback,
            state: Mutex::new((
                ScanProgress {
                    stage: ScanStage::Discovering,
                    candidates: 0,
                    entries: 0,
                    files_measured: 0,
                    bytes_measured: 0,
                },
                Instant::now(),
                false,
            )),
        }
    }
    // Callbacks run synchronously under this lock: never concurrently. They
    // should be short/nonblocking; cancelling the token is safe.
    pub fn stage(&self, stage: ScanStage) {
        let mut state = self.state.lock().unwrap();
        state.0.stage = stage;
        state.1 = Instant::now();
        state.2 = false;
        (self.callback)(state.0);
    }
    pub fn work(&self, candidates: u64, entries: u64, files: u64, bytes: u64) {
        let mut state = self.state.lock().unwrap();
        state.0.candidates = state.0.candidates.saturating_add(candidates);
        state.0.entries = state.0.entries.saturating_add(entries);
        state.0.files_measured = state.0.files_measured.saturating_add(files);
        state.0.bytes_measured = state.0.bytes_measured.saturating_add(bytes);
        // One first-work snapshot per stage, then at most ten per second.
        let meaningful_work = state.0.stage != ScanStage::Measuring || state.0.files_measured > 0;
        let first_work = !state.2 && meaningful_work;
        if first_work || state.1.elapsed() >= Duration::from_millis(100) {
            state.2 |= meaningful_work;
            state.1 = Instant::now();
            (self.callback)(state.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_budget_is_bounded_and_clones_keep_their_limit() {
        assert_eq!(
            ScanControl::default().candidate_limit(),
            MAX_SCAN_CANDIDATES
        );
        assert_eq!(
            ScanControl::default()
                .with_candidate_limit(0)
                .candidate_limit(),
            1
        );
        assert_eq!(
            ScanControl::default()
                .with_candidate_limit(usize::MAX)
                .candidate_limit(),
            MAX_SCAN_CANDIDATES
        );
        let control = ScanControl::default().with_candidate_limit(2000);
        assert_eq!(control.clone().candidate_limit(), 2000);
    }

    #[test]
    fn progress_throttles_bursts_but_keeps_exact_final_counters() {
        let control = ScanControl::default();
        let events = Mutex::new(Vec::new());
        let callback = |p| events.lock().unwrap().push(p);
        let progress = Progress::new(&control, &callback);
        progress.stage(ScanStage::Measuring);
        progress.work(0, 1, 1, 7);
        // Pin the throttle window in the future, without wall-clock sleeps.
        progress.state.lock().unwrap().1 = Instant::now() + Duration::from_secs(60);
        for _ in 0..10_000 {
            progress.work(0, 1, 1, 7);
        }
        progress.stage(ScanStage::Completed);
        let events = events.into_inner().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].files_measured, 10_001);
        assert_eq!(events[2].bytes_measured, 70_007);
    }

    #[test]
    fn merging_diagnostics_keeps_the_global_cap_and_counts_omissions() {
        let mut combined = ErrorLog::default();
        for _ in 0..MAX_SCAN_WORKERS {
            let mut worker = ErrorLog::default();
            for _ in 0..MAX_SCAN_ERRORS + 1 {
                worker.record(Path::new("fixture"), "error");
            }
            combined.append(worker);
        }
        assert_eq!(combined.errors.len(), MAX_SCAN_ERRORS);
        assert_eq!(
            combined.suppressed as usize,
            MAX_SCAN_WORKERS * (MAX_SCAN_ERRORS + 1) - MAX_SCAN_ERRORS
        );
    }
}

#[derive(Default)]
pub(crate) struct ErrorLog {
    pub errors: Vec<ScanError>,
    pub suppressed: u64,
}
impl ErrorLog {
    pub fn record(&mut self, path: &Path, error: impl ToString) {
        if self.errors.len() < MAX_SCAN_ERRORS {
            self.errors.push(ScanError {
                path: path.display().to_string(),
                message: error.to_string(),
            });
        } else {
            self.suppressed = self.suppressed.saturating_add(1);
        }
    }
    pub fn append(&mut self, other: Self) {
        self.suppressed = self.suppressed.saturating_add(other.suppressed);
        for error in other.errors {
            if self.errors.len() < MAX_SCAN_ERRORS {
                self.errors.push(error);
            } else {
                self.suppressed = self.suppressed.saturating_add(1);
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.suppressed == 0
    }
}
