---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry
kind: story
stage: done
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting
release_binding: 1.0.0
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

## Implementation notes

- Execution capability: highest, retained from feature ownership because current-reference lifetime, reconnect fencing, protocol privacy, and geometry provenance are future public evidence guarantees. Dispatch was direct-read and single-owner; local symbol/file probes fully resolved the integration surface, so no nested agent was used.
- Review weight: standard from the caller. Review is not applicable at this child-story checkpoint and remains feature-scoped.
- Files changed: `.work/active/stories/epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry.md`; `crates/krometrail-core/src/ports/{browser.rs,mod.rs}`; `crates/krometrail-cdp/src/control/{mod.rs,snapshot.rs}`; `crates/krometrail-cdp/src/session/{mod.rs,runtime.rs,reconnect.rs}`; `crates/krometrail-cdp/tests/temporal_evidence.rs`.
- Core correction: `ResolvedReferenceGeometry` now carries the one sampled `ObservedTime` beside normalized `resolved_at`, preserving current clocks separately and documenting that the value contains no source-frame identity, historical claim, or tracking provenance. The object-safe `CurrentReferenceGeometry` view delegates through one narrow `BrowserSessionPort` adapter seam; adapters without a live snapshot registry fail explicitly, while `ProductionSession` overrides the seam.
- Actor routing: `ProductionSession` sends a dedicated typed current-geometry command through the existing bounded supervision channel. The single session actor owns `PageControl`, exact supervisor state, `SnapshotRegistry`, and the active transport/session binding. Reconnect rejects rather than queues or replays the command; ended sessions and absent current connections return source-safe stale-reference guidance.
- Exact resolution: the path reuses the registry's target, snapshot generation, attachment generation, current document fingerprint, snapshot-node binding, described backend identity, live runtime-object, visibility, and finite non-zero border-quad checks under `VisibleGeometry`. Existing interaction-blocked facts remain ignored only for this visibility requirement, so a still-visible node that became inert/disabled remains readable while hidden, detached, missing, stale, and zero-area nodes fail.
- Geometry and timing: after exact resolution, the actor reads fresh `Page.getLayoutMetrics`, reuses the existing validated CSS layout-viewport decoder, subtracts the layout origin from document-quad bounds, and returns the un-clipped viewport-relative `CssRect`. It samples the monotonic clock exactly once after protocol reads and derives normalized session time from that same observation.
- Privacy/boundary: outputs contain only core session/target/reference IDs, attachment generation, neutral CSS geometry, and current clocks. Errors replace all transport/backend/runtime/session details with stable typed scope and fresh-snapshot recovery. No selector fallback, screenshot, browser-operation/MCP registry variant, recording/store call, durable geometry, natural range, or historical source-frame association was added.
- Tests added/updated: core object-safe session/narrow-port dispatch; exact registry refresh/reconnect/close/wrong-target/backing checks; reconnect no-replay/source-safety; scripted production actor tests for current blocked-visible geometry, negative un-clipped viewport coordinates, exactly-one clock sample, normalized timing, no selector/screenshot/historical payload, wrong session/target, navigation fingerprint change, refreshed generation, closed session, hidden/detached/zero-area state, malformed quad/layout, stable recovery, and private-ID redaction.
- Simplification: live-target snapshot retention was named once for ordinary operations and current geometry; synchronous target/generation/attachment/backing checks were extracted from the existing resolver for direct reuse and deterministic lifecycle tests; layout decoding reuses the existing `rect_from_viewport` authority rather than adding another protocol geometry parser.
- Discrepancies from design: the design sketch made `BrowserSessionPort` a direct supertrait of `CurrentReferenceGeometry`. That would force unrelated MCP test adapters outside the permitted write set and would not provide Rust 1.85 trait-object upcasting. The narrow trait instead blankets the session adapter seam while production still routes only through the exact actor. Routing lives in `session/runtime.rs`/`reconnect.rs` rather than `session/operations.rs` because this is deliberately not a browser operation and must remain available without registry/evidence dispatch. No other deviation.
- Adjacent issues parked: none.

## Verification evidence

- `rustup run 1.85.0 cargo fmt --all -- --check` — passed.
- `rustup run 1.85.0 cargo check -p krometrail-core -p krometrail-cdp --all-targets --locked` — passed.
- `rustup run 1.85.0 cargo test -p krometrail-core -p krometrail-cdp --all-targets --locked` — passed, 344 tests across core and CDP targets.
- `rustup run 1.85.0 cargo clippy -p krometrail-core -p krometrail-cdp --all-targets --locked -- -D warnings` — passed.
- `rustup run 1.85.0 cargo check --workspace --all-targets --locked` — passed as the required reverse-dependency check.
