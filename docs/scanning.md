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
- Unix identity reconciliation uses device/inode/type snapshots without holding
  one open descriptor per candidate. Observed identities are checked again.
- Windows uses bounded simultaneous identity handles rather than retaining all
  candidates' handles. Its pairwise comparison can cost O(n² × depth); native
  Windows execution must still be checked in CI.
- Discovery collects at most 10,000 candidates, including duplicate aliases
  before reconciliation. Exceeding this budget discards discovery and returns a
  partial report rather than weakening risk checks. Library callers can lower
  the limit with `ScanControl::with_candidate_limit`.
- Candidate count is bounded, but path lengths and traversal stacks are not
  comprehensively memory-bounded.
- Multiple independent callers can still start multiple scans: a process-wide
  application concurrency policy is future work.

These limits do not claim suitability for arbitrary millions-of-files inputs.
This does not remove TOCTOU windows or establish a globally atomic filesystem
snapshot. Further scalability work includes traversal budgets and faster Windows
identity indexing without weaker identity checks.

Unix identity lookup count is linear in candidates times path depth, but current
link-policy checks revisit ancestors for each identity. Total metadata operations
can therefore grow as O(candidates × depth²). The 2,000-candidate stress fixture
does not establish performance on arbitrarily deep directory trees.

## CLI adapter

`prune scan --workers N` accepts 1–16 (default 2). Interactive progress requires
both stdin and stderr to be terminals and is disabled for `--json`. Progress goes
to stderr through a bounded channel; stdout remains reserved for the report.
`q`, Esc and Ctrl+C request cooperative cancellation. The adapter joins the worker
before printing the partial report and returning exit status 130.

JSON/noninteractive scans do not install an interactive keyboard handler.
Ctrl+C in these modes retains OS behavior and need not produce a final report.

## Portability

Rule paths use `/` separators on every OS. Native Windows backslashes are
intentionally rejected by the rule grammar, not normalized after validation.
Windows execution and terminal cancellation need CI and manual testing; Unix
tests alone do not establish Windows correctness.
