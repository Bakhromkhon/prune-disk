# Module boundaries and safety gates

Status: design decision; not all modules described here exist yet.

Prune is a local application, not a distributed service. Extensibility means
separable responsibilities and testable interfaces, not network processes.

## Dependency direction

- Presentation (CLI/TUI and later Tauri) calls shared application use cases.
- Application scenarios depend on domain policies and narrow infrastructure ports.
- Local adapters implement filesystem access, rule loading and future persistence.
- A future executor isolates all move/restore/purge capabilities. Read-only builds
  should not require this capability.

The current two-crate workspace is not a claim that all these boundaries already
exist. Introduce modules first. Extract a `prune-local` crate when its port contracts
are concrete; do not mechanically split files into empty abstractions.

## Required boundaries

1. **Rules and path policy:** raw TOML is untrusted. Validation produces an opaque
   rule set; scanning must not accept raw rules as an authority.
2. **Discovery and measurement:** discovery enforces the allowed scope;
   measurement returns observations, not permission to remove an object.
3. **Scanner orchestration:** bounded work, cancellation and progress belong here,
   not in a particular UI.
4. **History:** immutable observations with their scope and completeness.
   A path not observed in a partial scan is not evidence it disappeared.
5. **Planner:** deterministic proposals with reasons and conservative policy.
   `Never` is an unconditional exclusion, regardless of a UI filter.
6. **Executor:** accepts a reviewed, revalidated plan, never arbitrary frontend
   paths or a deserialized scan report.
7. **Presentation:** escapes untrusted strings and formats measurements; it does
   not decide whether a destructive operation is authorized.

## Separate contracts

`ScanReport -> CleanupPlan -> ApprovedPlan -> ExecutionReceipt`

These are different entities. Approval binds to a specific immutable plan.
Imported JSON must be validated again; deserialization is not approval.
Version external DTOs independently of internal storage migrations.

## Write-feature gate

No move/delete/purge feature before:

- independent regression testing of traversal, links, reparse points and races;
- handle-relative platform-specific path resolution, or conservative refusal;
- a durable operation journal and crash recovery at every state transition;
- conflict-safe undo that never silently overwrites existing data;
- one writer at a time and idempotent recovery;
- tests for disk full, permission errors, cross-volume moves and cancellation;
- explicit separation of quarantine and purge: a rename on the same volume
  does not reclaim storage, and purge removes the local undo option.

Logical, allocated and actually reclaimed bytes must not be conflated.
Neither mtime nor a rule author's `safe` label proves data is disposable.
