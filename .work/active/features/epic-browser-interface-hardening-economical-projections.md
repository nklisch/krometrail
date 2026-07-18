---
id: epic-browser-interface-hardening-economical-projections
kind: feature
stage: review
tags: [agent-ux, browser]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Economical Agent Projections

## Brief

Make automatic live snapshots and default temporal bundles reliably small enough for routine agent use while retaining explicit full/canonical drill-down. The current live limits still allow a 32 KiB/96-node payload that dominates mutations, and temporal compaction is incorrectly coupled to snapshot/page-state preference fields so a request that omits those unrelated parts can receive the full temporal bundle.

Preserve acquisition and canonical evidence. Put all change in the MCP presentation layer, publish deterministic omission/count summaries, and test serialized size rather than assuming node or row counts imply ergonomic output.

## Source findings

- `idea-bound-compact-snapshot`
- `idea-compact-temporal-bundle`

## UI alignment

No UI surface; this is an MCP response-projection feature.

## Design decisions

- **Live budget**: lower automatic compact snapshots to 48 nodes and 12 KiB of serialized node JSON. This keeps a usable tree and actionable ancestry while materially reducing routine mutation payloads.
- **Temporal control**: add an additive `response.temporal` field with `compact` default and `full` opt-in. Temporal detail must not be inferred from `snapshot` or `page_state`.
- **Drill-down**: `snapshot: full`, `temporal: full`, canonical resources, and retained artifacts remain authoritative and unchanged.

## Architectural choice

Keep one acquisition/result model and project it at the shared MCP response boundary. Add a temporal-specific presentation preference rather than overloading unrelated structured fields. This is preferable to changing domain bundles (which would weaken retained evidence) or tool-specific booleans (which would fragment the common response surface).

## Implementation Units

### Unit 1: Bounded automatic snapshots

**Story**: `epic-browser-interface-hardening-economical-projections-bound-snapshots`

**File**: `crates/krometrail-mcp/src/response.rs`

```rust
const MAX_AUTOMATIC_SNAPSHOT_NODES: usize = 48;
const MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES: usize = 12 * 1024;

fn compact_snapshot(snapshot: PageSnapshot) -> Result<PageSnapshot, ResponseInvariantError>;
```

Keep actionable-first ancestor-preserving selection and exact omission accounting. Apply the tighter budget only to compact/automatic presentation.

**Acceptance criteria**:

- [ ] Large default post-action snapshots fit both 48-node and 12-KiB ceilings.
- [ ] Explicit full snapshots remain complete.
- [ ] Selected nodes retain valid preorder parents and exact omission counts.

### Unit 2: Temporal-specific compact projection

**Story**: `epic-browser-interface-hardening-economical-projections-compact-temporal`

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`

```rust
enum TemporalResponseDetail { Compact, Full }

struct ResponseProjectionRequest {
    temporal: TemporalResponseDetail,
    // existing fields retained
}

fn apply_temporal_projection(value: &mut Value, detail: TemporalResponseDetail)
    -> Result<(), ResponseInvariantError>;
```

Compact temporal output keeps range/header, counts, gap/warning summaries, artifact handles, and drill-down identifiers. It removes repeated per-frame/provenance structures already available through resources/full detail.

**Acceptance criteria**:

- [ ] Omitted snapshot/page-state fields do not disable temporal compaction.
- [ ] Default temporal bundles stay under a deterministic serialized-size regression ceiling for the reproduced multi-frame shape.
- [ ] `temporal: full` returns the existing bundle projection without loss.

## Implementation Order

1. Tighten and regress the live snapshot budget.
2. Add the additive temporal preference and decouple bundle projection.

## Simplification

- One common response preference owns temporal detail; remove the duplicated snapshot/page-state conditional from both temporal mapping paths.
- Retain one compact temporal helper rather than route-local summaries.

## Testing

- MCP response tests protect exact serialized budgets, full opt-in, schema generation, and independence among response preference fields.
- Existing resource/canonical bundle tests remain unchanged as compatibility evidence.

## Risks

Dense control surfaces can exceed 48 actionable nodes. Omission accounting remains explicit and full snapshot drill-down is preserved. The new response field is additive and defaults to the already documented low-cost behavior.

## Implementation summary

- Reduced automatic post-action snapshot presentation to 48 nodes and 12 KiB of serialized node JSON, preserving actionable ancestors, validated preorder structure, exact omission accounting, and explicit full snapshots.
- Added `response.temporal` with a compact default and full opt-in. Both temporal response mappings use this preference directly; snapshot and page-state detail cannot expand temporal responses.
- Compact temporal results retain range/header, quality and warning summaries, artifact handles, and canonical resources. `temporal: full` retains the prior bundle projection and explicit resource drill-down.

## Verification

- Passed: `cargo test -p krometrail-mcp --all-targets --locked` (63 tests), `cargo check -p krometrail-mcp --all-targets --locked`, and `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings`.
- MCP files pass direct `rustfmt --check`. The workspace-wide `cargo fmt --all -- --check` currently reports only concurrent, out-of-scope formatting in `crates/krometrail-cdp/src/control/snapshot.rs`.
