---
id: compact-live-observations-bound-snapshots
kind: story
stage: done
tags: [agent-ux, browser]
parent: compact-live-observations
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Bound automatic snapshots

## Checkpoint

Automatic post-action and batch-final snapshots preserve prioritized actionable context within exact presentation budgets and account for every omitted node; explicit inspection remains full.

## Acceptance evidence

- Node and serialized-byte ceilings are tested on a complex synthetic snapshot.
- Parent/reference invariants and explicit-surface behavior remain intact.

## Implementation notes

- Added MCP-presentation compaction only for `post_action` and `batch_final` live-observation roles. Explicit `snapshot_page` and `observe_live` continue through their unbounded response paths.
- Selection marks actionable nodes and their complete ancestor chains first, then fills remaining preorder context only when the parent is already selected. The reconstructed snapshot is revalidated through `PageSnapshot::new`.
- The 96-node and 32 KiB serialized-node-array ceilings are enforced together. Presentation omissions are checked and added exactly to acquisition-time `omitted_node_count`; already-bounded snapshots are returned unchanged.

## Verification

- Red regression: `cargo test -p krometrail-mcp response::tests::automatic_live_observations_bound_complex_snapshots_with_exact_omissions --locked` failed before implementation because the automatic limits/projection did not exist.
- `cargo test -p krometrail-mcp response::tests::automatic_live_observations_bound_complex_snapshots_with_exact_omissions --locked` — passed.
- `cargo test -p krometrail-mcp response::tests::explicit_snapshot_and_live_observation_keep_full_snapshots --locked` — passed.
- `cargo test -p krometrail-mcp response::tests::automatic_snapshot_below_limits_is_byte_equivalent --locked` — passed.
