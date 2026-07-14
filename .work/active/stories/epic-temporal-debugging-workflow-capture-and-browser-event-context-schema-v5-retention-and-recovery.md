---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery
kind: story
stage: implementing
tags: [browser, storage, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on:
  - epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy
  - epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Add Schema v5 Browser Event Retention and Recovery

## Checkpoint

After artifact publication lands schema v4, add the next contiguous event migration in `schema_v5.rs`. Atomically persist one structured sanitized event row, typed generic-timeline identity, global retention sequence, and browser-event usage entry. Add deterministic bounded reads, independent oldest-event eviction with unavailable-range tombstones, session deletion, live-page accounting, corruption handling, and idempotent startup reconciliation.

This story must never edit or claim `schema_v4.rs`. If committed HEAD has advanced beyond v4 before implementation, it uses the next contiguous version while retaining the artifact-schema dependency.

## Files

- `crates/krometrail-store/src/index/schema_v5.rs` (new; exclusive v5 ownership)
- `crates/krometrail-store/src/index/{migrations.rs,browser_events.rs,timeline.rs,retention.rs,maintenance.rs,deletion.rs,mod.rs}`
- `crates/krometrail-store/src/{recording.rs,recovery.rs,lib.rs}`
- `crates/krometrail-store/tests/{browser_events.rs,browser_event_recovery.rs,retention_small_budget.rs,sqlite_schema.rs}`

## Acceptance evidence

- Fresh and artifact-v4 stores migrate transactionally to contiguous v5; future versions refuse and failed v5 leaves v4 intact.
- Event ID/ordinal replay is idempotent only for byte-equivalent domain values; conflicts fail source-safely.
- Payload, projected query fields, typed timeline row, shared retention sequence, and usage commit in one bounded transaction.
- Range/filter/priority/nearest/cursor reads follow exact session-time/ordinal/ID ties and reject corrupt registry/projection values without leaking them.
- Artifact-first cleanup then compares unpinned segment and event retention sequence; event batches evict independently and leave coalesced unavailable tombstones.
- Event bytes are classified without double-counting SQLite live pages; freelist bytes are reusable accounting slack.
- Pins preserve source segments, not contextual events; session deletion removes all event/timeline/usage/tombstone state and prevents resurrection.
- Recovery repairs missing timeline/usage, tombstones recoverably corrupt rows, fails on unbounded identity/time corruption, and is a no-op on the second pass.

## Ordering

Depends on core contracts and the artifact schema/publication checkpoint. It is the only migration-registry write in this feature and unblocks range-context queries.