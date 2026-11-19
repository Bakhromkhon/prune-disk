# Roadmap

Prune is built in public. This roadmap is a direction, not a promise. Priorities
may change based on real user testing and platform evidence.

## v0.1 — X-ray (read-only foundation)

- [x] Rust workspace with `prune-core` and `prune-cli`
- [x] macOS and common developer-cache rules
- [x] TUI, table and JSON output
- [x] Validated rules and conservative path discovery
- [x] Cooperative cancellation and bounded scan controls
- [x] Atlas as the default local report viewer; previous viewer retained
- [x] CI on macOS, Windows and Linux

## v0.2 — Trustworthy history (still read-only)

- [x] Version 2 report with explicit completeness and privacy-safe scope metadata
- [x] v1 import remains supported; missing legacy completeness is `unknown`
- [x] Optional `--redact-home` for JSON sharing
- [ ] User-opt-in local SQLite history of metadata only
- [ ] Scope-aware `prune diff` for two complete, matching snapshots
- [ ] Clear UI for history retention, deletion and path sensitivity
- [ ] Time-to-live / stale-report labeling in Atlas (data freshness, not safety)
- [ ] Cross-platform history migration tests and backup/recovery tests

## v0.3 — Explainable plans (no file changes)

- [ ] Deterministic, side-effect-free `prune plan`
- [ ] Show reasons, exclusions, risk and uncertainty for every proposed item
- [ ] Report logical-size estimates, never guaranteed reclaimed bytes
- [ ] Plan can be inspected/exported but not executed
- [ ] Stable application/progress contract before a Tauri GUI
- [ ] User testing with people who do not use a terminal

## v0.4 — Desktop preview and platform coverage

- [ ] Read-only Tauri GUI over the shared Rust application API
- [ ] Verified rule coverage for Windows and Linux
- [ ] Windows reparse-point and long-path runtime tests
- [ ] Platform packaging and accessibility verification
- [ ] Import/export privacy review, localization and support diagnostics

## Conditional milestone — Cleanup executor (not scheduled)

No `clean`, move, quarantine or purge feature is enabled until every gate in
[`docs/module-boundaries.md`](docs/module-boundaries.md) and
[`docs/security.md`](docs/security.md) is met, including race-safe native path
resolution, a durable journal, crash recovery, conflict-safe undo, fault
injection and a separate security review. A same-volume quarantine move does
not free space. Permanent purge is distinct and irreversible. If those
guarantees cannot be met on a platform, that platform will refuse the operation.

## Explicitly deferred

- Automatic cleanup, schedules and autopilot
- Community-downloaded rule packs and rule auto-updates
- Privileged helpers and full-disk scanning
- Duplicate-content detection and secure wipe
- AI/cloud analysis or telemetry

Any future network feature would need an explicit product decision, documentation,
opt-in behavior and a distinct trust boundary.
