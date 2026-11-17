# Privacy

## Short version

Prune has **no telemetry**, **no accounts**, **no ads**, and **no network code** in its core. It reads file metadata locally, prints the result locally, and nothing leaves your machine unless you explicitly export or copy it.

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

`prune-core` has no network dependencies. CI checks the dependency graph and the core source for network APIs. A runtime network monitor (Little Snitch, LuLu, Wireshark) should show no connections from `prune`.

If a future feature needs the network (for example, checking for updates), it will be:

1. off by default;
2. explicitly documented;
3. opt-in;
4. built outside `prune-core` with a clear boundary.

We have no plans to add analytics or telemetry.

## Exporting data

`prune scan --json` writes a report to stdout. Paths can contain your username and other personal information. Treat exported reports as you would treat a screenshot of your filesystem.

## Contact

Privacy questions can be opened as a GitHub issue or discussed privately via the security advisory channel if they involve a vulnerability.
