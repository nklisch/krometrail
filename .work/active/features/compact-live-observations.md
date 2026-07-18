---
id: compact-live-observations
kind: feature
stage: review
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Compact live observations

## Brief

Keep automatic post-action observations useful without saturating an agent's context or repeating indistinguishable degradation. Creating a Wikipedia page with v1.0.3 returned a 403-node accessibility snapshot and roughly 48,000 tokens when the caller primarily needed the new target identity and current page state. Clicking a JavaScript-alert fixture correctly degraded observation while the dialog was open, but repeated the exact same `page_observation_failed` warning three times at both the response and diagnostic-log boundaries.

Bound automatic observation snapshots while preserving explicit drill-down through the snapshot tool and accurate omission accounting. Coalesce equivalent top-level observation warnings, or retain component identity when warnings are meaningfully distinct, so repetition always carries information.

## Simplification opportunity

Centralize automatic-observation response policy instead of letting each action or observation component independently expand snapshots and append equivalent warnings. Reuse the snapshot model's existing `omitted_node_count` and the shared response composer.

## Design decisions

- **Bounded surface**: compact automatic post-action and batch-final snapshots at the MCP presentation boundary. Explicit `snapshot_page` and user-requested `observe_live` retain their full existing behavior as drill-down surfaces.
- **Selection policy**: preserve actionable nodes and their ancestor context first, then fill remaining preorder context within both node and serialized-byte budgets. Never emit a child whose parent was omitted.
- **Omission truth**: add presentation-dropped nodes to the snapshot's existing `omitted_node_count`; do not mutate the CDP snapshot registry or reference generation.
- **Warning identity**: structural `KrometrailError` equality defines equivalence. Same code with different message, context, retry, or recovery remains distinct; exact clones are logged and exposed once.
- **Dispatch rationale**: one read-only generic explorer mapped the shared observation, snapshot, and response seams; the host verified the decoder and response tests.

## Architectural choice

Leave acquisition and domain snapshots unchanged, then project an agent-sized `PageSnapshot` only when the shared MCP response composer knows the observation role is automatic. This preserves stable explicit inspection and backing-reference registries while enforcing context bounds in the one place that owns model-facing presentation. Deduplicate warnings in `Projection::degrade_with_stage`, before both diagnostics logging and top-level accumulation.

Alternatives rejected:

- Lowering the global CDP decoder's 5,000-node/1 MiB limits would also truncate explicit `snapshot_page` and weaken the existing drill-down contract.
- Prefix-only truncation can discard the primary interactive controls on pages with large navigation or document trees.
- Adding component labels to cloned warnings would make three copies technically different while retaining context waste; nested component positions already explain which evidence is unavailable.

## Implementation Units

### Unit 1: Role-aware compact snapshot projection

**Story**: `compact-live-observations-bound-snapshots`

**Files**: `crates/krometrail-mcp/src/response.rs`

```rust
const MAX_AUTOMATIC_SNAPSHOT_NODES: usize = 96;
const MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;

fn project_live_observation(
    value: LiveObservation,
    role: ImageRole,
    step_index: Option<u32>,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError>;

fn compact_automatic_snapshot(snapshot: PageSnapshot) -> Result<PageSnapshot, ResponseInvariantError>;
```

**Implementation notes**:

- Invoke compaction for `PostAction` and `BatchFinal`, not `LiveObservation` or explicit `SnapshotPage`.
- If the snapshot already fits both budgets, return it byte-for-byte equivalent.
- Precompute actionable nodes plus their ancestor chains, select those in original preorder while budgets allow, then fill remaining nodes whose parent is selected. Reconstruct through `PageSnapshot::new` so preorder/reference invariants remain enforced.
- Budget serialized node JSON bytes, not only text payload, because field/structure overhead caused the reproduced context pressure.

**Acceptance criteria**:

- [ ] A 403-node automatic snapshot is bounded to at most 96 nodes/32 KiB of node JSON with exact presentation omission accounting.
- [ ] Actionable nodes are preferred over non-actionable prose while all emitted parent relationships remain valid.
- [ ] Explicit snapshot and explicit live-observation responses remain unprojected.

### Unit 2: Equivalent warning coalescing

**Story**: `compact-live-observations-deduplicate-warnings`

**Files**: `crates/krometrail-mcp/src/response.rs`

```rust
impl Projection {
    fn degrade_with_stage(&mut self, warnings: Vec<KrometrailError>, failure_stage: &str) {
        for warning in warnings {
            if self.warnings.contains(&warning) {
                continue;
            }
            // log once, then retain once
        }
    }
}
```

**Implementation notes**:

- Deduplicate before logging so the local diagnostic file mirrors the response signal.
- Preserve first-seen deterministic order.
- Do not deduplicate by error code alone.

**Acceptance criteria**:

- [ ] The dialog-blocked observation exposes and logs one `page_observation_failed` warning while each nested component remains explicitly unavailable.
- [ ] Distinct errors sharing one code remain separate and retain order.
- [ ] Capture-failure degradation still composes without duplicates.

## Implementation Order

1. Add snapshot compaction and role-aware projection tests.
2. Coalesce warnings in the shared projection and update dialog/batch regressions.

## Simplification

- Keep one full snapshot acquisition path and one presentation compactor rather than adding automatic modes throughout CDP control.
- Use existing snapshot omission and error identity contracts.
- Remove the test expectation that duplicated warnings are useful output.

## Testing

- MCP response tests protect serialized byte/node ceilings, actionable selection, parent validity, unchanged explicit surfaces, and exact omission count.
- Warning tests protect full-identity deduplication and distinct-error preservation.
- Existing CDP partial-observation tests continue protecting non-error degradation and are not weakened.

## Risks

- Dense pages can contain more actionable controls than the automatic budget. Deterministic preorder and explicit omission make that loss visible; callers can request `snapshot_page` for complete detail.
- Reconstructing a compact snapshot must not alter active registry bindings. Keeping compaction after domain execution and validating the reconstructed value isolates that risk.

## Implementation results

- `a0b814c` (`implement: compact-live-observations-bound-snapshots`) added role-aware response projection for automatic post-action and batch-final observations. It prioritizes actionable nodes and ancestor chains, fills valid preorder context under the remaining budget, revalidates through `PageSnapshot::new`, and adds exact presentation omissions without touching acquisition or reference registries.
- `9915aef` (`implement: compact-live-observations-deduplicate-warnings`) moved full-equality warning coalescing ahead of both tracing and accumulation. First-seen order is stable, cloned dialog/capture warnings collapse, and structurally distinct same-code warnings remain visible.
- Explicit `snapshot_page` and `observe_live` responses remain full. Foundation assertions remain current because this changes only the size policy for automatic MCP presentation, not snapshot acquisition or explicit inspection contracts.
- Implementation used the existing shared response seam directly; no additional registry, acquisition mode, or compatibility layer was introduced.

## Aggregate verification

- `cargo test -p krometrail-mcp --all-targets --locked` — passed, 35 tests.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- The focused regressions cover the reproduced 403-node shape, both automatic roles, both explicit drill-down surfaces, exact omission accounting, byte-equivalent small snapshots, cloned three-component dialog degradation, pre-log deduplication, distinct same-code warning identity, and duplicate capture-failure composition.
