---
id: epic-agent-browser-operation-page-observation-snapshot-references
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-page-observation
depends_on: [epic-agent-browser-operation-page-observation-operation-executor]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Build compact snapshots and generation-scoped references

## Checkpoint

Implement Unit 3 and the parent design's trickiest unit. Decode `Accessibility.getFullAXTree` into deterministic compact core nodes, assign one checked non-zero generation per target, atomically install exactly one active generation, and keep snapshot-local backend bindings private. Add the one shared resolver used by element screenshots now and verified interactions later.

The registry must bind generation to Krometrail target, current attachment generation, and a `Page.getFrameTree` main-frame/loader fingerprint. Resolution then verifies current fingerprint, backing node existence/connection, hidden/inert/disabled state, computed visibility, and non-zero finite `DOM.getBoxModel` geometry. Never guess a replacement by role/name/selector. A successful newer snapshot invalidates its predecessor; a failed snapshot leaves the prior active generation intact.

Keep fixed 5,000-node, 1 MiB text, and 32-properties-per-node bounds with exact omitted count. Preserve the parent's accessibility-property allowlist and one local actionability role/signal declaration. Additive fields/properties and new role strings must not terminate decoding. Selector lookup remains a one-shot weaker helper and never creates a generation identity.

## Required files

- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/session.rs`

## Acceptance evidence

- [ ] Output is bounded deterministic preorder with valid topology, exact truncation count, compact selected properties, and references only for actionable candidates with backend metadata.
- [ ] New snapshot, loader/navigation change, attachment/reconnect change, target closure, missing/replaced backing node, and generation mismatch all invalidate old references with refresh guidance.
- [ ] Connected but hidden/inert/disabled/geometry-less nodes fail as not actionable; no selector or name fallback occurs.
- [ ] Failed refresh cannot erase the previous valid generation; generation overflow fails instead of wrapping.
- [ ] Shadow/iframe nodes resolve only when their backing node belongs to the verified document/session; otherwise they remain non-actionable or fail explicitly.

## Ordering

Depends on `epic-agent-browser-operation-page-observation-operation-executor`. It is the reference-safety checkpoint required before element screenshots or later input actions.

## Implementation notes

- Added one per-target `SnapshotRegistry` owned by `PageControl`. Generations advance with checked non-zero arithmetic and install atomically only after complete AX decoding and core topology validation; a failed refresh leaves the prior active snapshot intact.
- Added deterministic AX graph traversal that flattens ignored/presentation nodes, preserves protocol child order, emits parent-before-child topology, ignores additive fields and unknown roles/properties, applies the fixed node/text/property bounds, and reports omitted retained nodes exactly.
- Kept the accessibility property allowlist and actionability roles/signals as local declarations. Only candidates with backing DOM identity and a supported role/signal receive generation-scoped references.
- Added the shared resolver: it checks target, active generation, attachment generation, main-frame/loader fingerprint, exact backing identity, live runtime connection/hidden/inert/disabled state, and finite non-zero box geometry. It never searches by role or name.
- Added a selector-only one-shot helper that does not mint durable references. Target pruning and attachment/document checks make closure, reconnect, and navigation invalidation explicit at the next operation boundary.

## Verification

- `cargo check -p krometrail-cdp --all-targets --locked` passed; resolver helpers are intentionally not called until the dependent screenshot checkpoint.
- `cargo test -p krometrail-cdp --lib --locked` — 72 tests passed, including additive AX decoding, ignored-node flattening, actionable binding, and quad bounds.
