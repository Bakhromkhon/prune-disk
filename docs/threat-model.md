# Threat model

## Assets

1. **User files** — documents, photos, source code, device backups, credentials.
2. **User privacy** — file paths, usernames, app usage, disk contents.
3. **System integrity** — the operating system and other applications.
4. **Trust in releases** — users must be able to verify that a binary matches the source.

## Adversaries

- A malicious or careless contributor submitting a rule or a pull request.
- A compromised dependency or GitHub Action.
- A malicious rule pack distributed outside the repository.
- A local attacker with the same user account.
- A well-intentioned bug in Prune itself.

## Threats and mitigations

| Threat | Impact | Mitigation | Status |
| --- | --- | --- | --- |
| Accidental deletion by a scanner bug | Data loss | v0.1 is read-only; future writes go through quarantine and undo; path validation; integration tests | Partial (v0.1 read-only) |
| Malicious rule escapes the home directory | Data loss | Strict schema; home-only paths; no `..`; broad roots rejected; rules cannot execute code | Yes |
| Symlink points outside a cache | Data loss | Symlinks are never followed; future deletion re-checks the path | Yes for scan |
| TOCTOU: file changes after scan | Wrong file removed | Re-stat before future writes; compare size, mtime, and inode/device where available | Planned |
| Malicious dependency | Code execution, data theft | Minimal dependency tree; committed lockfile; `cargo-deny`; review; no network crates in core | Partial |
| Compromised GitHub Action | Release tampering | Least-privilege tokens; pin actions to SHAs before v1.0; protected release environment; attestations | Planned |
| Malicious pull request | Backdoor in a release | Review; CODEOWNERS on sensitive paths; CI; no secrets in PR workflows; release from tags only | Partial |
| Hidden telemetry in a release binary | Privacy loss | No network code in core; CI check; reproducible builds; attestations; source review | Partial |
| Privilege escalation | Full system compromise | No sudo, admin, setuid, or shell execution | Yes |
| Re-creation after cleanup | User frustration | Every rule documents how data is recreated; receipts show what happened | Partial |

## Out of scope

- An attacker who already has administrator/root access.
- A compromised operating system, kernel, or firmware.
- Physical access to an unlocked machine.
- Denial of service from scanning a deliberately enormous or hostile filesystem.
- Third-party apps that store data in unexpected locations and do not document it.

## Residual risks

- A rule can be technically correct but still surprising to a user. Risk levels and explanations reduce this, but cannot eliminate it.
- v0.1 reads metadata only, but metadata itself is sensitive. Reports are local and must be handled carefully when shared.
- The project is young. Review coverage and release hardening will improve before any write path ships.

## Review triggers

A new security review is required before:

- the first deletion or move code lands;
- a privileged helper is introduced;
- an auto-update mechanism is added;
- a network feature is added;
- a community rule registry accepts untrusted rules automatically.
