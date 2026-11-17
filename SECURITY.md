# Security Policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | ✅ |

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting:

<https://github.com/Bakhromkhon/prune-disk/security/advisories/new>

Do **not** open a public issue for a security problem. We will acknowledge the report, investigate it, and coordinate a fix and disclosure.

## Security guarantees in v0.1

- **Read-only.** v0.1 has no code path that deletes, moves, renames, or writes to user files.
- **Network-free core.** `prune-core` has no network dependencies. CI rejects network-capable crates and network APIs in the core source.
- **No telemetry.** There are no analytics SDKs, accounts, crash reporters, or remote calls.
- **No privilege escalation.** Prune never uses sudo, admin rights, setuid helpers, or shell commands.
- **Validated rules.** Rule paths must be inside the user's home directory, must be specific, cannot contain `..`, and symlinks are never followed.
- **Memory safety.** The core crate uses `#![forbid(unsafe_code)]`.

## Supply chain

- `Cargo.lock` is committed and reviewed.
- `cargo-deny` checks advisories, licenses, wildcard dependencies, and sources.
- GitHub Actions use least-privilege permissions.
- Releases are built from tagged source in CI, not on a maintainer laptop.
- Release artifacts will ship with checksums, an SBOM, and build provenance attestations.

## Scope

In scope:

- The scanner, rule engine, CLI, and future GUI.
- Rule path validation and any code that can write, move, or delete files.
- Build, release, and update mechanisms.
- Dependency and GitHub Actions supply chain.

Out of scope:

- Vulnerabilities in third-party applications whose caches Prune scans.
- Attacks that already require administrator/root access or physical access.
- Denial of service caused by scanning an intentionally enormous filesystem.

## Disclosure

We aim to acknowledge reports within 72 hours and to publish a fix within 90 days, sooner when possible. Credit is given unless you prefer to remain anonymous.

## More details

- [docs/security.md](docs/security.md)
- [docs/threat-model.md](docs/threat-model.md)
- [docs/privacy.md](docs/privacy.md)
