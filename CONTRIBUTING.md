# Contributing to Prune

Thanks for helping. Prune is built in public, and the most valuable contributions right now are **rules**, **tests**, and **platform knowledge**.

## Code of Conduct

By participating, you agree to follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Setup

Requirements:

- Rust stable (1.85 or newer). Install it with [rustup](https://rustup.rs/).
- Git.

```bash
git clone https://github.com/Bakhromkhon/prune-disk.git
cd prune-disk
cargo build
cargo run -p prune-cli -- scan --no-tui
```

## Project layout

```text
crates/prune-core/   Scanner, rule model, validation, risk model
crates/prune-cli/    CLI, TUI, JSON output
rules/macos/         macOS-specific rules
rules/common/        Cross-platform developer cache rules
docs/                Architecture, security, privacy, rule spec
```

## Checks

Before opening a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-no-network.sh
```

CI runs the same checks on macOS, Windows, and Linux.

## Adding a rule

1. Pick the right directory: `rules/macos/`, `rules/windows/`, `rules/linux/`, or `rules/common/`.
2. Create a TOML file. Use an existing rule as a template.
3. Choose the risk honestly:
   - `safe` — regenerated automatically, no user data.
   - `low` — re-downloadable or low-impact.
   - `medium` — app state or archives that may matter.
   - `high` — user data, device backups, databases, containers.
   - `never` — must not be removed by Prune.
4. Set `recreate` to `automatic`, `download`, `rebuild`, `manual`, `impossible`, or `unknown`.
5. Verify the path on a real machine. Never guess a destructive path.
6. Run `cargo test --workspace` and `cargo run -p prune-cli -- rules validate`.
7. Open a PR using the rule request template if you are not sure about the path.

Safety invariants for rule paths:

- Must be inside the user's home directory (`~/...` or an absolute path under the home directory).
- Must be specific. `~/Library/*` is rejected.
- No `..` or `.` components.
- No environment variables yet.
- No symlink following.
- No code, commands, or shell snippets in rules.

## Safety rules for code

- Do not add network calls to `prune-core`.
- Do not add telemetry, analytics, crash reporting, or accounts.
- Do not use sudo, admin rights, setuid, or shell execution.
- Do not add `unsafe` to `prune-core`.
- Any future deletion code must go through quarantine, a write-ahead manifest, and an undo path. It must be reviewed separately.
- New dependencies need a reason. Prefer the standard library and small, audited crates.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
feat(core): add Xcode DerivedData rule
fix(cli): handle empty scan results
docs: explain the risk model
```

Sign off your commits with the Developer Certificate of Origin:

```bash
git commit -s
```

## Pull requests

- Keep PRs focused.
- Add or update tests.
- Update docs if behavior changes.
- Describe the safety impact in the PR template.
- Do not include personal paths, usernames, or machine-specific output in fixtures.

## Good first issues

Look for the `good first issue` label. Rule requests, additional fixtures, and documentation improvements are great starting points.
