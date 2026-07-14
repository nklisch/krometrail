---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-qualification
kind: story
stage: done
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries-query-service-composition]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify operation-to-query temporal resolution

## Checkpoint

Qualify the complete application seam with real SQLite/segment storage and scripted browser execution: returned standalone and batch interaction anchors must be immediately queryable through the same `TemporalQuery` service, with deterministic natural-anchor, retention, gap, redaction, and failure behavior.

## Acceptance evidence

- [ ] One fixture resolves session-time, wall-clock, source-frame, interaction, latest-interaction, navigation, and marker anchors and returns exact requested/resolved ranges and effective options.
- [ ] Implicit interaction resolution proves 150 ms before start through observed/completed plus 250 ms trailing context.
- [ ] Tied frame/timeline/interaction times prove capture-ordinal and documented UUID tie ordering.
- [ ] Fully evicted, contiguous edge-partial, internal-hole, never-captured, session-deleted, wrong-session/target, and gap include/reject cases produce the designed outcomes.
- [ ] Migration and readback preserve anchor-only page operations, exact action records, parent batch IDs, navigation/marker points, and source-safe decode failure.
- [ ] Standalone and per-step batch operations are queryable before success; delayed/failing sinks prove publication/stop ordering.
- [ ] Persisted fill, dialog, and upload records exclude fill text, prompt text, and directory components while preserving permitted sanitized metadata.
- [ ] The old “interaction anchors are always absent” test is removed or replaced; no low-value wrapper/SQL/MCP tests are added.
- [ ] Locked format, workspace check/test, and Clippy gates pass.

## Ordering

Depends on the fully composed production path and is the final implementation checkpoint before feature-level review.

## Implementation notes

- Execution capability: highest; retained by caller for the integrated core/store/CDP/root qualification surface.
- Review weight: standard, from the autopilot caller; child checkpoint review is not applicable.
- Files changed: `crates/krometrail-core/src/timeline/{query.rs,range.rs}` tests/invariant tightening; `crates/krometrail-store/tests/temporal_queries.rs`; `crates/krometrail-cdp/{Cargo.toml,tests/temporal_evidence.rs}` plus `Cargo.lock`; one existing process-observation test stabilization in `crates/krometrail-cdp/tests/support/chrome.rs`.
- Tests added/updated: all seven application anchors; bounded whole-millisecond Serde and both policies; exact implicit 150 ms/250 ms range; tied capture/timeline ordering; gap include/reject; wrong-scope interaction/source/navigation/marker; complete, full/edge/internal eviction, never-captured, and deletion outcomes; actual browser-result→same-`RecordingStore` standalone and two-step batch queries; fill/dialog/upload persisted redaction; Rust 1.85 workspace gates.
- Semantics qualified: source-frame identities are metadata-only and capture-ordinal ordered; complete requests retain exact requested bounds; only explicit contiguous evicted edges become partial; eviction tombstones remain separate from declared capture gaps; standalone and every batch interaction is queryable before publication; persistence failure stops default batches and cannot be reported as success.
- Simplification: the CDP test adds `krometrail-store` only as a dev dependency to exercise the real composed sink/query seam; no production test cache, MCP persistence, or copied range model was introduced.
- Discrepancies from design: the internal-hole query fixture directly establishes post-eviction SQLite state so all retention classifications can coexist deterministically; actual segment deletion/coalescing is separately exercised through `RecordingStore`'s real removal worker. The existing `/proc` reference test exposed a scheduler visibility race under Rust 1.85 and now waits for a bounded one-second condition instead of assuming immediate process-table visibility.
- Adjacent issues parked: none.
- Verification: under `rustc/cargo 1.85.0`, `cargo fmt --all -- --check`, locked workspace all-target check, locked workspace all-target test, and locked workspace all-target Clippy with `-D warnings` all pass. Focused qualification: core 75 passed; store temporal queries 3 passed; CDP temporal evidence 7 passed; full CDP 221 passed before the final integrated additions. No live-Chrome claim; opt-in tests remained disabled.
