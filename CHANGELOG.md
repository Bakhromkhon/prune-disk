# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- Require an opaque validated rule set at the scanner API boundary.
- Restrict path patterns to literal components and a terminal wildcard; reject
  traversal spellings, link ancestors and Windows reparse points during discovery.
- Reconcile overlapping candidates using filesystem identity before risk filtering.
- Escape terminal control/bidi characters, including argument errors.
- Make the network denylist guard fail closed on tooling errors.

### Changed

- Split path policy, discovery and measurement into separate internal modules.
- Report logical byte accounting and partial scans explicitly.
- Improve TUI warnings, sorting, narrow layouts, scrolling and Ctrl+C handling.
- Pin the tested Rust baseline to 1.100.0 and use locked CI builds.

These remain read-only protections, not a race-proof filesystem sandbox.

## [0.1.0] - 2026-11-17

### Added

- Read-only disk scanner with a parallel size aggregation engine.
- 17 built-in rules for Xcode, Homebrew, browsers, Docker, iOS backups, Trash, logs, npm, pnpm, Yarn, Cargo, pip, and Go.
- Interactive TUI, plain table, and JSON output.
- `prune scan`, `prune rules list`, `prune rules validate`, and `prune doctor`.
- Custom rule directories via `--rules-dir`.
- Risk model (`safe`, `low`, `medium`, `high`, `never`) and re-creation model.
- Rule path validation that restricts rules to specific paths inside the user's home directory.

### Security

- `#![forbid(unsafe_code)]` in `prune-core`.
- No network dependencies in `prune-core`; CI checks the dependency graph and source.
- No telemetry, analytics, accounts, sudo, or shell execution.
- Read-only v0.1: no delete, move, or write code path for user files.
