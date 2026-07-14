---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index
kind: story
stage: implementing
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
