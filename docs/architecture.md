# Architecture

## Goals

- Make disk usage understandable before making it removable.
- Keep the engine cross-platform from day one.
- Make safety a property of the architecture, not a promise in the README.
- Keep the core small, testable, and free of network code.

## Non-goals for v0.1

- Deleting files.
- Elevating privileges.
- Running vendor uninstallers.
- Scanning the entire filesystem by default.
- Storing file contents.

## Workspace

```text
crates/prune-core/   Scanner, rule model, validation, risk model
crates/prune-cli/    CLI, TUI, JSON output
rules/macos/         macOS-specific rules
rules/common/        Cross-platform developer cache rules
docs/                Architecture, security, privacy, rule spec
```

The core is a library. The CLI is a thin shell around it. A future Tauri GUI will call the same core through a local IPC boundary, so the scanning and safety logic stays in one place.

```text
┌──────────────────────────────────────┐
│ GUI (Tauri + React)                  │
│ TUI (ratatui) / CLI (clap)           │
└──────────────────┬───────────────────┘
                   │ local IPC / library
┌──────────────────▼───────────────────┐
│ prune-core                           │
│ scanner · rules · risk · planner     │
│ quarantine · history (SQLite)        │
└──────────────────┬───────────────────┘
                   │
┌──────────────────▼───────────────────┐
│ OS adapters: macOS · Windows · Linux │
└──────────────────────────────────────┘
```

## Scan pipeline (v0.1)

1. Load built-in rules at compile time, or custom rules from `--rules-dir`.
2. Validate every rule:
   - unique ID;
   - valid OS values;
   - at least one path;
   - every path is inside the home directory and specific.
3. Filter rules by OS, category, and maximum risk.
4. Expand `~` to the home directory.
5. Match paths with `glob`.
6. Deduplicate exact paths.
7. For each match, compute size in parallel:
   - directories are walked recursively;
   - symlinks are skipped;
   - filesystem boundaries are not crossed;
   - per-entry errors are collected, not fatal.
8. Sort findings by size.
9. Return a `ScanReport` with totals and warnings.

The scan only reads metadata (`symlink_metadata`, directory entries). It does not open file contents.

## Rule model

Rules are TOML data. They describe:

- a stable ID and human name;
- a plain-language description;
- supported operating systems;
- a category;
- a risk level;
- a re-creation path;
- one or more path patterns.

A rule cannot execute code, run a shell command, or request privileges. This keeps community rules auditable and limits the blast radius of a bad rule.

See [rule-spec.md](rule-spec.md).

## Safety invariants

- `#![forbid(unsafe_code)]` in `prune-core`.
- No network dependencies in `prune-core`.
- No sudo, admin, setuid, or shell execution.
- Rule paths must stay inside the user's home directory.
- Broad roots, `.` and `..` components, and environment variables are rejected in v0.1.
- Symlinks are never followed.
- Scanning does not cross filesystem boundaries.
- v0.1 has no write path for user files.

## Future components

### History

A local SQLite database will store metadata snapshots: path, size, modification time, rule ID, and scan ID. It will not store file contents. `prune diff` will compare scans and answer "what grew?".

### Planner

The planner will take a target ("free 50 GB") and a risk tolerance, then rank candidates by safety, size, and staleness. It will explain the trade-offs and never execute automatically in the first release.

### Quarantine

Before any deletion, files will be moved into a local quarantine directory with a write-ahead manifest. A crash during the move must be recoverable. `prune undo` will restore from the manifest. Quarantine retention will be configurable (default 30 days).

### GUI

Tauri 2 with a React frontend. The GUI will not get direct filesystem access; it will call the core through a narrow command interface. No remote content, no CDN, strict CSP.

## Cross-platform plan

The core is written against platform-neutral APIs and `std::env::consts::OS`. Platform-specific paths live in rules, not in code. Future OS adapters may handle:

- Windows: long paths, locked files, registry uninstall entries, Recycle Bin.
- Linux: XDG directories, package manager caches, Flatpak/Snap.
- macOS: APFS snapshots, Full Disk Access, notarization.

System-level cleanup that requires administrator/root is deliberately out of scope for v1. A future privileged helper, if any, must have a narrow API and a separate security review.
