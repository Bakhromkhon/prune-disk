# Controlled read-only scans

The application API exposes `scan_with_control(options, control, on_progress)`.
The convenience `scan(options)` uses default controls without a progress consumer.
Neither function authorizes subsequent file deletion.

## Cancellation

`ScanControl` clones share a one-way cancellation token. Cancelled reports have
`cancelled: true` and `partial: true`; they must not be interpreted as complete
snapshots for history or planning. Cancellation during discovery prevents
measurement of the incomplete candidate set, preserving conservative overlap
reconciliation.

Cancellation is cooperative. It cannot interrupt a filesystem syscall already
in progress. A caller must wait for the scan worker to stop before considering
the operation finished.

## Progress

Snapshots identify discovery, measurement, completion or cancellation. Counters
describe observed work, not a percentage or reclaimable storage. Measured bytes
may include findings below the output size filter. Stage changes and the first
useful work can emit immediately; subsequent work updates are throttled.

Callbacks are synchronous and serialized. They must be short and nonblocking:
UI adapters should use a bounded channel and `try_send`, dropping intermediate
snapshots if the display is behind. The final report, not a lossy progress event,
is the authoritative result.

## Resource limits and remaining work

- Default two measurement workers; accepted worker count is clamped to 1–16.
- Each scan uses its own worker pool rather than configuring global Rayon state.
- Diagnostics are capped; `suppressed_errors` records omitted diagnostics.
- Identity handles are bounded. Exceeding the budget rejects discovery rather
  than weakening alias/risk checks. This can exclude otherwise legitimate scans.
- Candidate and traversal collections are not yet comprehensively memory-bounded.
- Multiple independent callers can still start multiple scans: a process-wide
  application concurrency policy is future work.

These limits do not claim suitability for arbitrary millions-of-files inputs.
The next scalability step is a bounded discovery/reconciliation design that
preserves filesystem identity and conservative risk handling without retaining
an unbounded set of open handles.

## Portability

Rule paths use `/` separators on every OS. Native Windows backslashes are
intentionally rejected by the rule grammar, not normalized after validation.
Windows execution and terminal cancellation need CI and manual testing; Unix
tests alone do not establish Windows correctness.
