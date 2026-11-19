# Scan report contract

The report is an observation, not a cleanup authorization. JSON schema version
and any local-history database migration version are independent.

## Version 2

The scanner emits `schema_version: 2`. It keeps the report local/portable and
does not serialize the trusted root path.

- `completeness`: `complete`, `partial`, or `cancelled` for new scans.
- `scope`: root **kind**, effective filters, measurement threshold and a
  versioned SHA-256 fingerprint of applicable validated rules.
- `byte_accounting`: `logical`; this is not allocated or guaranteed reclaimed
  space.
- `path_encoding`: omitted for ordinary UTF-8 paths; `lossy_utf8` warns that a
  non-UTF-8 OS path is represented for display and cannot round-trip.
- `u64` values are JSON numbers through JavaScript's safe integer limit and
  decimal strings above it. Consumers must preserve exact decimal values.
- Non-UTF-8 POSIX paths are serialized as display-only lossy strings and carry
  `path_encoding: "lossy_utf8"`; they cannot reconstruct the original path bytes.

The rule fingerprint is **not a signature, source attestation, or trust score**.
It includes canonical applicable rules before category/risk filters, because
excluded descendants can affect overlap/risk reconciliation. The hash
canonicalization is domain-separated and versioned (`sha256-v1`). It excludes
the trusted root path. Custom rules may still be guessed from their fingerprint;
do not treat the digest as anonymization.

`scope.root_kind` is descriptive. Public JSON intentionally carries no private
root identity, and `scope.comparable` is always false. Matching
platform/rules/filters alone cannot prove two reports came from the same home
directory, machine, or user. Future local history must establish identity in
its private store before offering a diff.

`completeness: complete` means all configured rules and filters completed with
no scan diagnostics. It does **not** mean the entire disk was scanned or that
all disk contents are represented. Partial/cancelled reports must not be used
to infer that an unlisted path disappeared.

## Version 1 compatibility

The viewer accepts schema v1. Since the v1 field set changed while the version
number remained 1, completeness is presence-aware:

| v1 evidence | Normalized status |
| --- | --- |
| `cancelled: true` | `cancelled` |
| errors, suppressed errors, a partial finding, or `partial: true` | `partial` |
| explicit `partial: false` and no contrary evidence | `complete` |
| missing `partial` and no contrary evidence | `unknown` |

An unknown v1 report is visibly marked unknown and is not evidence of a complete
scan. Its scope is absent/incomparable. Never silently backfill a missing legacy
flag with `false`.

## Comparison rules for future history/diff

No diff is currently implemented. A future comparator must require two complete
snapshots and exact equality of platform, trusted-root identity held locally,
rule fingerprint, categories, inclusive `max_risk` ceiling, minimum-size
threshold and byte-accounting semantics. An exported scope descriptor alone
does not establish trusted-root identity. Partial/cancelled/unknown reports may
be inspected but cannot prove an item disappeared.

## Safety details

- `Risk::safe` is an authored rule label, never permission to delete.
- A rule digest proves neither provenance nor safety.
- Paths in exports are sensitive. `--redact-home` masks recognized home
  prefixes in serialized string values, but is not anonymization; inspect the
  resulting JSON before sharing.
- A lossy path is display-only. An imported report never authorizes filesystem
  access or execution.
