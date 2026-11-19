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
5. Resolve literal paths and an optional terminal `/*` through controlled discovery.
6. Reconcile overlapping candidates conservatively before measurement.
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
- Discovery rejects observed symlink ancestors and reparse points; this is not
  race-proof resolution. Write operations require stronger platform-specific APIs.
- Recursive measurement is constrained by filesystem boundaries relative to its
  discovered root; this is not a blanket guarantee about all mounts under home.
- v0.1 has no write path for user files.

## Future components

### History

A future, opt-in local SQLite database may store metadata snapshots: paths relative
to the trusted home boundary, sizes, modification times, rule IDs and scan IDs.
It must not store file contents, and the UI must explain that paths are sensitive
metadata and provide a way to erase the history. JSON schema v2 now records
completeness, effective filters and a canonical rule fingerprint without exporting
the root path. That public scope is descriptive, not proof of root identity;
v1 reports with missing completeness remain `unknown`. A diff must compare only
complete reports and establish identical private root identity locally. Partial,
cancelled, unknown or cross-root reports cannot establish that an item disappeared.

### Planner

The first planner milestone is read-only: consume a complete report and explicit
policy, then produce a deterministic proposal with reasons and exclusions.
`risk` is an authored estimate, not deletion authority; `never` is always excluded.
Any target is a logical-size estimate, not guaranteed freed space. No plan is
executed until a separate executor passes the write-feature gates.

### Quarantine

Quarantine, undo and purge are conditional future features, not a commitment or
part of the next read-only milestones. If ever implemented, they require a durable
write-ahead journal, crash recovery, race-safe platform operations and conflict-safe
undo. A same-volume quarantine move does not free disk space; permanent purge is
separate and irreversible.

### GUI

Tauri 2 is a future packaging option after the report/application contract settles.
The first GUI remains read-only and uses the same Rust application API; the
frontend gets no direct filesystem authority. No remote content, no CDN, strict CSP.

## Cross-platform plan

The core is written against platform-neutral APIs and `std::env::consts::OS`. Platform-specific paths live in rules, not in code. Future OS adapters may handle:

- Windows: long paths, locked files, registry uninstall entries, Recycle Bin.
- Linux: XDG directories, package manager caches, Flatpak/Snap.
- macOS: APFS snapshots, Full Disk Access, notarization.

System-level cleanup that requires administrator/root is deliberately out of scope for v1. A future privileged helper, if any, must have a narrow API and a separate security review.
