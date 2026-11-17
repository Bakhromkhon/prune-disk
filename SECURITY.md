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
- **No intentional network functionality.** Source and dependency checks flag selected network libraries and APIs. These checks are heuristics, not a sandbox or proof of absence of network activity.
- **No telemetry.** There are no analytics SDKs, accounts, crash reporters, or remote calls.
- **No privilege escalation.** Prune never uses sudo, admin rights, setuid helpers, or shell commands.
- **Path restrictions.** Rules are intended to address specific locations under the home directory. Path validation and no-follow checks are defense in depth, not a filesystem sandbox: concurrent replacement of filesystem objects remains a risk. No move/delete feature may rely on these read-only checks alone.
- **Unsafe-code policy.** The core crate uses `#![forbid(unsafe_code)]`; this does not prohibit unsafe code in dependencies.

## Supply chain

- `Cargo.lock` is committed and reviewed.
- `cargo-deny` checks advisories, licenses, wildcard dependencies, and sources.
- GitHub Actions use least-privilege permissions.
- A tag-based release pipeline has not been implemented. Current CI builds and tests source but does not publish binary releases.
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
