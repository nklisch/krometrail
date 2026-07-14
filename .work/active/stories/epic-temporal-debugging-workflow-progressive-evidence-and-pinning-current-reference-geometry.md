---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry
kind: story
stage: implementing
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Resolve Exact Current Reference Geometry

## Checkpoint

Implement the narrow inward `CurrentReferenceGeometry` port through the existing browser-session actor and `SnapshotRegistry`. Resolve an exact live `NodeReference` with the current session/target/snapshot generation/attachment/document/backing node, obtain visible finite geometry, read fresh layout viewport origin, and return a viewport-relative `CssRect` plus current timing and generation provenance.

The operation is current-only and read-only. It creates no browser-operation/MCP variant, screenshot, durable geometry record, selector fallback, natural-anchor re-resolution, or historical element identity. Browser protocol reads happen without a recording mutation gate.

## Files

- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/progressive.rs`
- `crates/krometrail-cdp/src/control/{snapshot.rs,mod.rs}`
- `crates/krometrail-cdp/src/session/{mod.rs,operations.rs}`
- `crates/krometrail-cdp/tests/temporal_evidence.rs`
- focused existing control/session tests

## Acceptance evidence

- Exact session, target, generation, node, attachment, current document fingerprint, backing identity, visibility, and geometry checks reuse the current resolver.
- Wrong scope, refresh, navigation, reconnect, target close, detached/hidden node, missing snapshot, and malformed layout/quad return stable source-safe failures with fresh-snapshot guidance.
- Visible disabled/inert nodes remain geometry-readable through `VisibleGeometry`; hidden or zero-area nodes remain rejected.
- Returned geometry is un-clipped viewport CSS with reference, attachment generation, and `resolved_at`; no backend/transport/CDP type escapes.
- Tests demonstrate that a current reference is sampled once and cannot be interpreted as a historical source-frame identity.

## Ordering

Depends on the core contracts (transitively through the coherent-store checkpoint). Progressive service composition consumes this port after it and the store authority are complete.
