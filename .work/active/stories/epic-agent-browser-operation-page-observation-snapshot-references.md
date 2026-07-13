---
id: epic-agent-browser-operation-page-observation-snapshot-references
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-page-observation
depends_on: [epic-agent-browser-operation-page-observation-operation-executor]
release_binding: null
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
