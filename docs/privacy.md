# Privacy

## Short version

Prune has **no telemetry**, **no accounts**, and **no ads**. Source review found no intentional network functionality; static dependency/source checks are heuristics, not proof of runtime network isolation. It reads metadata locally, prints results locally, and nothing leaves your machine unless you explicitly export or copy it.

## What v0.1 reads

- File and directory names under the paths matched by rules.
- File sizes, modification times, and file types (file, directory, symlink).
- Non-fatal filesystem errors such as permission denied.

It does **not**:

- open file contents;
- upload paths, sizes, or errors;
- read browser history, passwords, or keychain data;
- access the network.

## What v0.1 stores

Nothing persistent. v0.1 has no database and no cache. Scan results exist only in memory and in the output you request (terminal, TUI, or `--json`).

Your shell may keep a history of commands you run. That is outside Prune's control.

## Future local data

Planned features will store data locally:

- **History database** — metadata snapshots (path, size, modification time) to power `prune diff`. No file contents.
- **Quarantine** — files you chose to remove, kept locally so `prune undo` can restore them.
- **Receipts** — local JSON/Markdown records of a cleanup.

These files will live in the platform's application data directory. They will never be uploaded by Prune. The documentation will explain how to inspect and delete them.

## Network

Prune has no intentional network functionality. CI checks selected dependency names and source patterns; this heuristic cannot prove the absence of every network capability. Runtime observation is a separate check, and network-mounted filesystems may cause OS-level traffic.

If a future feature needs the network (for example, checking for updates), it will be:

1. off by default;
2. explicitly documented;
3. opt-in;
4. built outside `prune-core` with a clear boundary.

We have no plans to add analytics or telemetry.

## Exporting data

`prune scan --json` writes a report to stdout. Paths can contain your username and other personal information. Treat exported reports as you would treat a screenshot of your filesystem.

For sharing, `prune scan --json --redact-home` replaces recognized home-directory
prefixes with `~` in JSON string values. This is **path masking, not anonymization**:
filenames, rule descriptions, errors, and other values may still identify you.
Review the output before sharing it. Without the option, JSON output remains
unchanged for compatibility.

See [report-schema.md](report-schema.md) for schema versions, completeness and
scope semantics.

## Contact

Privacy questions can be opened as a GitHub issue or discussed privately via the security advisory channel if they involve a vulnerability.
