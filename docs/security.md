# Security

## Security model

Prune treats the user's files as the most important asset and the release pipeline as part of the attack surface. The design favors small, auditable components over convenience.

## Guarantees in v0.1

| Property | How it is enforced |
| --- | --- |
| Read-only | No delete, move, rename, or write path for user files exists |
| Network guard | Static checks flag selected network dependencies/APIs; no runtime isolation is claimed |
| No telemetry | No analytics, accounts, crash reporting, or remote calls |
| No privilege escalation | No sudo, admin, setuid, or shell execution |
| Path policy | Specific home-scoped paths are required; read-only checks do not eliminate filesystem races |
| Unsafe-code policy | `#![forbid(unsafe_code)]` in `prune-core`, not in all dependencies |
| Link policy | Link traversal must be rejected during discovery; concurrent replacement requires stronger OS-specific measures before any write feature |

## Trust boundaries

- **Rule data** is untrusted input. It is parsed into a strict schema and validated before use.
- **File metadata** is untrusted input. Errors are collected; a broken entry does not abort the scan.
- **The CLI** is trusted code but has no more privilege than the user.
- **The release pipeline** is a supply-chain boundary. Builds must come from tagged source in CI.

## Supply chain

- `Cargo.lock` is committed.
- `cargo-deny` checks advisories, licenses, wildcard dependencies, and sources.
- GitHub Actions use least-privilege `permissions`.
- Third-party actions should be pinned to commit SHAs before v1.0.
- Release artifacts will include checksums, an SBOM, and GitHub build provenance attestations.
- Auto-update is intentionally absent in v0.1. A future updater must be signed and opt-in.

## Verification

You can run the static network denylist guard:

```bash
cargo tree -p prune-core --prefix none | grep -E '^(reqwest|hyper|ureq|isahc|curl|surf|attohttpc|tokio-tungstenite|tungstenite|quinn|h2|rustls|native-tls|openssl)( v|$)'
./scripts/check-no-network.sh
```

The script is a denylist-based static guard, not proof that a binary cannot use the network. Runtime observation with Little Snitch, LuLu, or Wireshark is a separate check. Access to network-mounted filesystems may itself cause OS-level network traffic.

## Future write paths

Any future code that deletes or moves user files must:

1. Be isolated in a small module with a narrow API.
2. Validate every path again immediately before the operation.
3. Move files to quarantine instead of deleting them directly.
4. Write a manifest before the move (write-ahead log) and recover from crashes.
5. Verify size and modification time since the scan.
6. Refuse to cross filesystem boundaries or follow symlinks.
7. Ship with integration tests for success, failure, crash recovery, and undo.
8. Receive a separate security review before release.

## What we do not claim

- Prune cannot protect against a user who intentionally deletes important files after being warned.
- Prune cannot fix a compromised operating system or a malicious local administrator.
- Prune cannot guarantee that a third-party app will not recreate a cache immediately.
- A public repository is not automatically safe; review, branch protection, and release provenance matter.

## Reporting

Use GitHub private vulnerability reporting: <https://github.com/Bakhromkhon/prune-disk/security/advisories/new>.
