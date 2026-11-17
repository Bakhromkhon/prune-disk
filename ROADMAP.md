# Roadmap

Prune is built in public. This roadmap is a direction, not a promise. Priorities can change when real users and contributors report what hurts most.

## v0.1 — X-ray (current)

- [x] Rust workspace with `prune-core` and `prune-cli`
- [x] Read-only scanner with parallel size aggregation
- [x] 17 built-in rules for macOS and cross-platform developer caches
- [x] TUI, plain table, and JSON output
- [x] Rule validation and path safety guard
- [x] CI on macOS, Windows, and Linux
- [x] No-network dependency check

## v0.2 — Janitor

- [ ] Local SQLite history of metadata snapshots
- [ ] `prune diff` — what grew since the last scan
- [ ] `prune plan --need 50GB` — goal-based cleanup plan
- [ ] Quarantine with a write-ahead manifest
- [ ] `prune clean` and `prune undo`
- [ ] Cleanup receipts (JSON and Markdown)
- [ ] Integration tests for quarantine and undo

## v0.3 — Desktop

- [ ] Tauri GUI for macOS
- [ ] Treemap and category views
- [ ] Plan review screen with risk explanations
- [ ] Cleanup receipt screen with one-click undo
- [ ] Weekly "what grew" report
- [ ] Menu bar presence

## v0.4 — Cross-platform

- [ ] Windows rules and packaging (MSI/EXE)
- [ ] Linux rules and packaging (deb/rpm/AppImage)
- [ ] Platform-specific app uninstall helpers
- [ ] Full CI matrix for scanners and installers
- [ ] Homebrew tap, Scoop, and winget manifests

## v0.5 — Autopilot

- [ ] Policy engine ("keep caches under 10 GB", "clean stale node_modules")
- [ ] Scheduled read-only scans and notifications
- [ ] Opt-in autopilot with a strict allowlist
- [ ] Community rule packs with tests and review

## v1.0 — Stable

- [ ] Security review of all write paths
- [ ] Code signing and notarization
- [ ] Reproducible builds and release attestations
- [ ] Complete documentation and rule coverage
- [ ] Stable JSON schema and CLI contract

## Beyond v1.0

- Duplicate finder with safe defaults
- Secure wipe (separate, explicit workflow)
- Startup and background item manager
- AI/MCP assistant for natural-language disk questions
- System-level cleanup through a narrow, audited helper (only if it can be done safely)
