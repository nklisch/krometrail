---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-range-context-query-service
kind: story
stage: done
tags: [browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: [epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Build Capture Quality and Event Context Query

## Checkpoint

Add one `TemporalContextQuery` over one already `ResolvedRange`. Derive exact frame availability, bounds, cadence, warnings, declared-gap summary, retention warnings, and persisted capture status/generations from metadata. Query the same sanitized event store in compact focus-aware or verbose cursor mode with exact clipping, filtering, ties, limits, drop/retention warnings, and no wall-time or causal guesses.

## Files

- `crates/krometrail-core/src/timeline/context.rs` (new)
- `crates/krometrail-core/src/timeline/mod.rs`
- `crates/krometrail-core/src/ports/browser_events.rs`
- `crates/krometrail-core/src/{error.rs,lib.rs}`
- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/tests/range_context.rs` (new)

## Acceptance evidence

- Metadata exactly matches resolved frame identity/scope/order; 0/1/many-frame edges, tied times, warning aggregation, and 20,000-frame ceiling are explicit.
- Cadence returns exact min/nearest-rank median/p95/max adjacent session-time deltas; gap duration uses clipped unions and never infers loss.
- Capture status includes the retained sample at/before start plus at most 128 transitions/generations; missing/evicted status warns.
- Optional clips intersect only the resolved retained range; at most 16 focus times must lie inside it; native source/wall clocks never join.
- Compact mode ranks errors/failures, HTTP status, navigation/dialog, then nearest focus distance, with deterministic ties and final chronological presentation.
- Verbose pages (maximum 1,000) use strict scope/time/ordinal/ID cursors without repeats or omissions.
- Collection gaps and retention/corruption unavailable ranges remain visible regardless of class/severity filters and report truncation explicitly.
- Visual epoch counting is absent because artifact generation owns that exact contract.

## Ordering

Depends on the durable v5 source/query adapter. Root integration joins this service with the independently implemented CDP domain authority.

## Implementation notes

- Execution capability: direct inline implementation. The checkpoint is one cohesive core policy plus one store adapter; local reads resolved the interfaces and the caller limited scope to this story, so no implementation fan-out was used.
- Review weight: standard by project default; independent review is not applicable to this child-story checkpoint.
- Files changed: `crates/krometrail-core/src/timeline/context.rs`, timeline/core exports, validated browser-event read types, `crates/krometrail-store/src/recording.rs`, and `crates/krometrail-store/tests/range_context.rs`.
- Tests added: validated request/filter/limit/cursor Serde; clip/focus/frame caps; exact frame identity/order and source-safe corruption; tied/short cadence and nearest-rank quantiles; overlapping/point gaps and warning summaries; compact priority/focus/dedup ordering; equal-time cursor pagination; status start/end/generation/cap/missing/unavailable behavior; filter-independent collection/unavailable evidence and truncation; event eviction with pinned frames; and concurrent session deletion through the mutation gate.
- Simplification: the core service consumes the existing metadata-only `FrameSource` and semantic `BrowserEventSource` directly. No query-specific SQL, natural-anchor path, frame payload read, visual epoch model, FPS/p99 calculation, or second event vocabulary was introduced.
- Discrepancies from design: `error.rs` needed no change because the existing stable error vocabulary and context/retry/recovery fields cover the query boundary. Collection gaps use a bounded Operational-class scan through the designed semantic source; if other operational events consume the cap, the result reports conservative explicit truncation rather than adding another adapter query surface.
- Adjacent issues parked: none.

## Implemented decisions

- `TemporalContextRequest` owns one revalidated `ResolvedRange`; contained clips, canonical unique filters, compact/chronological selection, scoped cursors, the 20,000-frame ceiling, and at most 16 contained focus times validate before any source read. It has no anchor or resolver dependency.
- `TemporalContextService` re-reads metadata in the exact resolved ID order and verifies scope, retained times, increasing ordinals, and nondecreasing session time. Capture quality copies requested/retained bounds, gaps, and retention warnings; aggregates each frame-warning kind once per frame; computes exact adjacent cadence; and clips/unions declared gaps without inferring loss.
- Capture status reports the retained sample at/before the effective start, ordered transitions through the end, and final retained state/generation. Missing status, any potentially status-bearing unavailable tombstone, and the conservative 128-sample cap are explicit warnings.
- Compact selection reads at most four priority candidates per result slot plus two predecessor/successor rows per focus, deduplicates IDs, ranks priority then minimum session-time distance then the stable event tuple, and presents results chronologically. Its reason is explicitly correlation metadata, not causality.
- Chronological selection uses the validated selector-bearing cursor and probes one bounded continuation row so `next_cursor` is exact even at the 1,000-row maximum. Matched and returned counts are separate.
- Collection gaps and unavailable ranges are loaded independently of the semantic filter. Both paths are bounded; cap pressure produces explicit warnings. `RecordingStore` holds its mutation gate across all frame/event reads and invokes the core service with its index authorities, avoiding a reentrant store call.
- Source failures are replaced with stable scope/range-only messages and recovery guidance. Concurrent deletion either completes after a coherent response or causes a scoped `NotFound`; malformed frame/event projections produce scoped, source-safe `PersistenceFailed`.

## Verification evidence

Rust 1.85 verification ran in an isolated detached worktree based on `f5e3056` with only this checkpoint's cached patch applied, covering the affected core/store packages:

- `cargo fmt --package krometrail-core --package krometrail-store -- --check` — passed.
- `cargo check -p krometrail-core -p krometrail-store --all-targets --locked` — passed.
- `cargo test -p krometrail-core -p krometrail-store --all-targets --locked` — passed; 91 core unit tests, 27 store unit tests, all existing store integration targets, and 10 focused range-context tests.
- `cargo clippy -p krometrail-core -p krometrail-store --all-targets --locked -- -D warnings` — passed.

No CDP, root composition, MCP, artifact/temporal-vision implementation, migration/schema, foundation documentation, sibling item, parent-feature transition, or `.work/bin/work-view` change is included.