//! Interactive scan progress lives on stderr, never on the report stream.
use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use prune_core::{
    ScanControl, ScanOptions, ScanProgress, ScanReport, ScanStage, scan_with_control,
};
use std::io::{self, Write};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::Duration;

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        // Arm rollback before the fallible operation, including partial failure.
        let guard = Self;
        enable_raw_mode()?;
        Ok(guard)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

// Also cancels on unwinding before the scope implicitly joins its worker.
struct CancelOnDrop<'a>(&'a ScanControl);

impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn send_progress(sender: &SyncSender<ScanProgress>, progress: ScanProgress) {
    // A slow terminal must never stall the scanner. Capacity is one snapshot.
    let _ = sender.try_send(progress);
}

fn format_progress(progress: ScanProgress) -> String {
    let stage = match progress.stage {
        ScanStage::Discovering => "Discovering",
        ScanStage::Measuring => "Measuring",
        ScanStage::Completed => "Completed",
        ScanStage::Cancelled => "Cancelled",
    };
    format!(
        "{stage}: {} candidates, {} entries, {} files, {} logical bytes observed (not reclaimable)",
        progress.candidates, progress.entries, progress.files_measured, progress.bytes_measured
    )
}

fn cancel_event(event: Event) -> bool {
    matches!(event, Event::Key(key)
        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
            || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'C')))))
}

/// Always cancel and join on event/output failure; no detached scanner survives.
fn with_worker<T: Send>(
    control: &ScanControl,
    work: impl FnOnce(SyncSender<ScanProgress>) -> T + Send,
    mut tick: impl FnMut(&Receiver<ScanProgress>) -> Result<bool>,
) -> Result<T> {
    thread::scope(|scope| {
        let (sender, receiver) = sync_channel(1);
        let worker = scope.spawn(move || work(sender));
        let cancel_on_unwind = CancelOnDrop(control);
        let events = (|| {
            while !worker.is_finished() {
                if tick(&receiver)? {
                    control.cancel();
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        })();
        if events.is_err() {
            control.cancel();
        }
        let result = worker.join().map_err(|_| anyhow!("scan worker panicked"));
        // Do not mark a successful, completed scan's shared token cancelled.
        std::mem::forget(cancel_on_unwind);
        events?;
        result
    })
}

/// Caller must ensure stdin and stderr are terminals and JSON is disabled.
pub fn run(options: &ScanOptions, control: &ScanControl) -> Result<ScanReport> {
    let _raw_mode = RawModeGuard::enter()?;
    let mut stderr = io::stderr().lock();
    write!(stderr, "Scanning (q/Esc/Ctrl+C cancels)…\r\n")?;
    stderr.flush()?;
    with_worker(
        control,
        |sender| scan_with_control(options, control, |p| send_progress(&sender, p)),
        |receiver| {
            if let Ok(progress) = receiver.try_recv() {
                // Only fixed labels and integer counters: no untrusted paths.
                write!(stderr, "{}\r\n", format_progress(progress))?;
                stderr.flush()?;
            }
            if event::poll(Duration::from_millis(50))? {
                return Ok(cancel_event(event::read()?));
            }
            Ok(false)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn progress(stage: ScanStage) -> ScanProgress {
        ScanProgress {
            stage,
            candidates: 3,
            entries: 9,
            files_measured: 7,
            bytes_measured: 123,
        }
    }

    #[test]
    fn formatting_uses_observed_counters_not_percentages() {
        for stage in [
            ScanStage::Discovering,
            ScanStage::Measuring,
            ScanStage::Completed,
            ScanStage::Cancelled,
        ] {
            let text = format_progress(progress(stage));
            assert!(text.starts_with(&format!("{stage:?}:")));
            assert!(text.contains("3 candidates, 9 entries, 7 files, 123 logical bytes"));
            assert!(text.contains("not reclaimable"));
            assert!(!text.contains('%'));
            assert!(!text.contains('\x1b'));
        }
    }

    #[test]
    fn full_or_disconnected_channel_never_blocks_callback() {
        let (sender, receiver) = sync_channel(1);
        for _ in 0..10_000 {
            send_progress(&sender, progress(ScanStage::Measuring));
        }
        assert_eq!(receiver.try_iter().count(), 1);
        drop(receiver);
        send_progress(&sender, progress(ScanStage::Completed));
    }

    #[test]
    fn cancellation_keys_require_correct_modifiers_and_ignore_release() {
        for code in [KeyCode::Char('q'), KeyCode::Esc] {
            assert!(cancel_event(Event::Key(KeyEvent::new(
                code,
                KeyModifiers::NONE
            ))));
        }
        assert!(cancel_event(Event::Key(KeyEvent::new(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL
        ))));
        assert!(!cancel_event(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::ALT
        ))));
        let mut key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert!(!cancel_event(Event::Key(key)));
    }

    #[test]
    fn event_error_and_cancel_both_join_worker_before_returning() {
        for fail in [false, true] {
            let control = ScanControl::default();
            let finished = AtomicBool::new(false);
            let result = with_worker(
                &control,
                |_| {
                    while !control.is_cancelled() {
                        thread::yield_now();
                    }
                    finished.store(true, Ordering::Release);
                },
                |_| {
                    if fail {
                        Err(anyhow!("event failed"))
                    } else {
                        Ok(true)
                    }
                },
            );
            assert_eq!(result.is_err(), fail);
            assert!(control.is_cancelled());
            assert!(finished.load(Ordering::Acquire));
        }
    }

    #[test]
    fn completed_worker_does_not_cancel_control() {
        let control = ScanControl::default();
        assert_eq!(with_worker(&control, |_| 42, |_| Ok(false)).unwrap(), 42);
        assert!(!control.is_cancelled());
    }

    #[test]
    fn cancellation_returns_explicit_partial_core_report() {
        let control = ScanControl::default();
        let report = with_worker(
            &control,
            |sender| {
                // Wait for the main thread to request cancellation deterministically.
                while !control.is_cancelled() {
                    thread::yield_now();
                }
                scan_with_control(&ScanOptions::default(), &control, |p| {
                    send_progress(&sender, p)
                })
            },
            |_| Ok(true),
        )
        .unwrap();
        assert!(report.cancelled);
        assert!(report.partial);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn event_panic_cancels_before_scope_joins() {
        let control = ScanControl::default();
        let finished = AtomicBool::new(false);
        let result = std::panic::catch_unwind(|| {
            let _ = with_worker(
                &control,
                |_| {
                    while !control.is_cancelled() {
                        thread::yield_now();
                    }
                    finished.store(true, Ordering::Release);
                },
                |_| panic!("event panic"),
            );
        });
        assert!(result.is_err());
        assert!(finished.load(Ordering::Acquire));
    }
}
