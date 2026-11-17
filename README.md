# Prune

> Make room. Keep what matters.

**Prune is an open-source disk cleaner with a memory, a plan, and an undo button.**

Today it is a **read-only scanner**. It shows where your space went, explains what each finding is, and never deletes anything. The safe cleanup flow — quarantine, receipts, undo — is the next milestone.

[![CI](https://github.com/Bakhromkhon/prune-disk/actions/workflows/ci.yml/badge.svg)](https://github.com/Bakhromkhon/prune-disk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
![No telemetry](https://img.shields.io/badge/telemetry-none-brightgreen)
![No sudo](https://img.shields.io/badge/sudo-never-brightgreen)

## Why another disk cleaner?

Most cleaners show a list of "junk" and a big Delete button. That model is opaque and risky: you do not know what will break, what will come back, or whether you can undo it.

Prune is built around three questions:

1. **What grew?** A local history of metadata snapshots will show what changed since last week.
2. **What is safe?** Every finding has a category, a risk level, a re-creation path, and a plain-language explanation.
3. **Can I undo it?** Cleanup will move files into a local quarantine with a manifest, so `prune undo` works.

## Status: v0.1 "X-ray" — read-only

- 17 built-in rules for macOS and cross-platform developer caches
- Parallel scan with recursive size aggregation
- Interactive TUI, plain table, and JSON output
- `prune rules list` and `prune rules validate`
- No deletion, no network, no sudo, no telemetry

## Quick start

Requires the current Rust stable toolchain.

```bash
git clone https://github.com/Bakhromkhon/prune-disk.git
cd prune-disk
cargo install --path crates/prune-cli

prune doctor
prune scan
```

Example output:

```text
Prune 0.1.0 — read-only scan
Platform: macos · Findings: 10 · Files: 20612 · Total: 6.97 GB

SIZE       RISK    CATEGORY    NAME                           PATH
5.19 GB    safe    developer   npm Cache                      ~/.npm/_cacache
794.13 MB  safe    browser     Google Chrome Cache            ~/Library/Caches/Google/Chrome/Default
482.34 MB  safe    developer   Homebrew Cache                 ~/Library/Caches/Homebrew/downloads
181.18 MB  safe    developer   pip Cache                      ~/Library/Caches/pip
102.64 MB  low     system      User Logs                      ~/Library/Logs/CreativeCloud
...
```

## Usage

```bash
prune scan                     # TUI when stdout is a terminal
prune scan --no-tui            # plain table, good for scripts and CI
prune scan --json              # machine-readable report
prune scan --min-size 100MB    # ignore smaller findings
prune scan --category developer
prune scan --max-risk safe     # only safe findings
prune scan --workers 2         # measurement workers, accepted range 1–16
prune rules list
prune rules validate
prune rules list --json
prune doctor
```

Custom rules can be loaded from a directory:

```bash
prune scan --rules-dir ./my-rules
```

Every custom rule is validated before it is used. In v0.1, rule paths must be inside your home directory and specific; broad roots and `..` traversal are rejected.

Interactive scans show progress on stderr; press `q`, Esc or Ctrl+C to cancel.
Cancellation prints a partial report and exits with status 130. JSON and
noninteractive scans do not show progress; OS signal termination in these modes
does not guarantee a final report. See [scanning limits](docs/scanning.md).

## Visual report preview

An offline report viewer is available in [`preview/`](preview/README.md).
It starts with clearly labelled fictional demo data and can import a local
`prune scan --json` report. It does not scan, move or delete files and is not yet
a native desktop application.

```bash
python3 -m http.server 4173 --bind 127.0.0.1 --directory preview
# Open http://127.0.0.1:4173
```

Imported reports stay in browser memory; do not commit reports containing personal
paths to this repository.

## Safety guarantees

- **Read-only.** v0.1 has no delete code path.
- **No intentional network functionality.** CI checks selected dependency names and source patterns. This is an additional review guard, not runtime network isolation.
- **No telemetry.** No analytics, no accounts, no crash reporting, no remote calls.
- **No sudo.** Prune never asks for administrator or root privileges and never runs shell commands.
- **Rules are data, not code.** A rule cannot execute a command or elevate privileges.
- **Path policy.** Read-only path restrictions are being hardened against traversal and link-based escapes. They are not a sandbox against concurrent filesystem changes and must not be reused as authorization for deletion.
- **`#![forbid(unsafe_code)]`** in the core crate (not its dependencies).

## What is next

| Version | Theme | Highlights |
| --- | --- | --- |
| v0.1 | X-ray | Read-only scan, rules, TUI/JSON |
| v0.2 | Janitor | History/diff, "free X GB" planner, quarantine, `undo` |
| v0.3 | Desktop | Tauri GUI, treemap, receipts, weekly report |
| v0.4 | Cross-platform | Windows and Linux rules, installers, CI matrix |
| v0.5 | Autopilot | Policies, schedules, community rule packs |
| v1.0 | Stable | Signing, audit, polish, docs |

See [ROADMAP.md](ROADMAP.md) for details.

## Contributing

Contributions are welcome — especially new rules, platform knowledge, and tests. Start with [CONTRIBUTING.md](CONTRIBUTING.md) and look for `good first issue`.

The most useful contribution right now is a **rule request**: tell us which app leaves large caches on your machine and how you verified the path.

## Security

Please report vulnerabilities privately via GitHub Security Advisories. See [SECURITY.md](SECURITY.md) and [docs/threat-model.md](docs/threat-model.md).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

**No telemetry. No sudo. No regrets.**
