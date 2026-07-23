---
id: feature-perf-wait-snapshot-pipeline-opt-3
kind: story
stage: done
tags: [perf]
parent: feature-perf-wait-snapshot-pipeline
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Typed AX decode from the owned response Value

Optimization 3 of the parent feature — transport-independent decode win.
`decode_ax_tree_with_ids` (`snapshot.rs:2399`) traverses `serde_json::Value`
manually: `HashMap<&str,&Value>` by_id, `HashSet<&str>` children, per-node
repeated string-keyed `.get()` probes, per-node `to_owned` of four strings.
68.39 ms @ 50k, superlinear. See the parent feature body (Optimization 3),
including the transport finding that fixes why we do NOT add a typed transport
method.

## Scope

- Define serde `Deserialize` wire structs for `Accessibility.getFullAXTree`
  (`AxTreeResponse` / `AxNodeWire` / `AxValueWire` / `AxPropertyWire`) and
  consume the **owned** `ax_response` `Value` via `serde_json::from_value`
  (moves String fields out instead of cloning).
- Change `decode_ax_tree_with_ids` (and the `#[cfg(test)] decode_ax_tree`
  helper) to take an owned `Value`; `capture_snapshot_for_frame` already owns
  it from `send_raw`.
- Build `nodeId -> index` maps once; resolve `childIds` through indices; run the
  same preorder `visit` preserving every domain rule — ignored/`none`/
  `presentation` skip, actionability, `MAX_SNAPSHOT_NODES` /
  `MAX_SNAPSHOT_TEXT_BYTES` caps, `backendDOMNodeId` → `SnapshotNodeId`
  assignment, `seen_backends` retain, frame filtering, and the
  different-document stale check.
- Respect `validated-wire-contracts`: wire structs are permissive shapes; all
  domain validation stays in the decode/domain layer, not in serde. The generic
  `send_raw` / `CdpTransport` contract is unchanged; DOM decode stays untyped
  (measured linear, not a bottleneck).

## Files

- `crates/krometrail-cdp/src/control/snapshot.rs` — wire structs, `Decoder`,
  `decode_ax_tree_with_ids`, `capture_snapshot_for_frame` call site,
  `decode_ax_tree` test helper.

## Acceptance criteria

- [ ] `perf_decode_ax_50k` shows ≥2× vs the 68.39 ms baseline (goal ~20 ms).
- [ ] Per-node `to_owned` clones eliminated (Strings moved out of the owned
      `Value`).
- [ ] Every existing AX-decode test passes unchanged: frame filtering,
      cross-origin/oopif rejection, caps/omission, actionability, backend-id
      stability, structural web area.

## Implementation notes

Implemented permissive serde AX wire structs over the owned response,
integer-indexed child assembly, and vector-backed visitation while retaining
frame filtering, actionability, identity reuse, caps, and omission behavior.
The release decode benchmark measured approximately 34.1 ms/op versus the
68.39 ms design baseline; this is just above the scaffold's ≤34 ms target on
the current host.
