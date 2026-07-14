---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index
kind: story
stage: done
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries-core-query-contracts]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Persist temporal anchors and eviction availability

## Checkpoint

Add transactional SQLite v3 storage for the existing `InteractionAnchor` and optional exact `InteractionRecord`, generic interaction/navigation/marker timeline uniqueness, deterministic latest-interaction reads, metadata-only frame queries, and compact eviction-range memory. Implement the sink on `RecordingStore` under its existing mutation gate.

## Required contract

- `interactions` stores typed identity/scope/operation/timing columns plus optional validated `record_json`; page operations remain anchor-only and browser actions retain their exact sanitized record.
- One operation-evidence transaction writes the interaction projection, one boundary observation per distinct timing point, and an optional successful explicit-navigation observation.
- Exact replay by interaction ID is idempotent; conflicting identity reuse or record/anchor mismatch is `PersistenceFailed`.
- Latest interaction orders effective observation, completion, dispatch, start, and UUID bytes descending within one exact session/target.
- Segment eviction records/coalesces the removed segment's frame-time interval before deleting frame metadata. Ordinary eviction preserves compact anchors; session deletion removes anchors/tombstones with the session.
- Production generic timeline writes pass through `RecordingStore` so marker writes cannot race session deletion.

## Acceptance evidence

- [ ] Clean and v2 databases migrate atomically to v3; future versions still refuse.
- [ ] Anchor-only and action-record rows round-trip through validated core constructors, including parent-batch and redacted parameter fields.
- [ ] Equal-time latest ordering and generic timeline ordering are insertion-order independent.
- [ ] Evicted ranges coalesce, survive segment removal, classify fully/partially evicted evidence, and disappear on session deletion.
- [ ] SQL/decode/corruption failures remain source-safe and never log record JSON.

## Ordering

Depends on the core contracts. It provides the durable sink/read implementation required before CDP can fence operation success.

## Implementation notes

- Execution capability: highest; retained by caller because migration, retention deletion, and exact evidence replay share one transaction boundary.
- Review weight: standard, from the autopilot caller; child checkpoint review is not applicable.
- Files changed: `crates/krometrail-core/src/browser/{interaction.rs,operation.rs}`; `crates/krometrail-store/src/index/{deletion.rs,frames.rs,interactions.rs,migrations.rs,mod.rs,range.rs,schema_v3.rs,timeline.rs}`; `crates/krometrail-store/src/recording.rs`; store range/schema/index tests.
- Tests added/updated: fresh/v2→v3/future-version migration; anchor-only and exact optional action-record round trip; idempotent replay; conflicting ID and corrupted JSON source-safe failure; latest equal-time UUID ordering; atomic boundary/navigation timeline projection; coalesced eviction memory and session-deletion cleanup; durable interaction range resolution replacing the former deliberate missing-anchor assertion.
- Semantics delivered: transactional v3 `interactions` and `evicted_frame_ranges`; exact typed reconstruction with record/anchor proof; deterministic latest ordering; one generic timeline uniqueness authority with exact-replay validation; segment eviction records and coalesces frame-time tombstones before metadata deletion; `RecordingStore` is the mutation-gated interaction and generic timeline writer.
- Simplification: added `InteractionRecord::anchor()` and registry-derived `BrowserOperationKind::from_stable_name()` so persistence does not copy either model or variant list.
- Discrepancies from design: v3 migration removes pre-v3 duplicate generic marker/navigation/boundary rows before installing unique indexes, preserving the earliest exact row so valid v2 databases migrate rather than failing on historical duplicate writes. This is an additive cleanup inside the same migration transaction.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; `cargo check -p krometrail-store --all-targets --locked`; `cargo test -p krometrail-store --all-targets --locked` (72 passed across 10 suites); `cargo clippy -p krometrail-store --all-targets --locked -- -D warnings`.
