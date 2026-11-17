# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
