---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery
kind: story
stage: done
tags: [browser, storage, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on:
  - epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy
  - epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication
release_binding: 1.0.0
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

## Implemented decisions

- Schema v5 is one contiguous transactional migration after artifact v4. It adds one strict sanitized payload table, typed timeline-reference uniqueness, unavailable-range tombstones, shared retention sequencing, and only the range/filter/priority/retention indexes needed by bounded semantic reads. Generic timeline insertion now rejects browser events so the atomic event path remains authoritative.
- `BrowserEventSink` and the object-safe `BrowserEventSource` are implemented by `RecordingStore`. Batch persistence validates the core event again, accepts only exact ID/ordinal replay, and commits payload, projections, typed timeline identity, usage, and sequence together. The pre-transaction usage snapshot keeps the immediate transaction bounded to at most the batch plus a 256-row/1-MiB event eviction.
- Chronological pages use strict `(session_time, ordinal, event_id)` cursors. Class/severity filters, compact-priority candidates, bounded predecessor/successor candidates, capture-status samples, and unavailable ranges all decode through the core registry and revalidate timeline identity before returning evidence.
- Browser-event accounting uses exact payload/projection bytes plus a documented 256-byte row/index allowance. SQLite index usage is checkpointed live pages minus classified event bytes; freelist pages are reported separately as reusable accounting slack. The open-segment allowance now applies only after no older evictable artifact, event, or sealed segment remains.
- Cleanup remains artifact-first, then compares the shared retention sequence of the oldest unpinned segment and oldest event. Event removal is metadata-only, bounded, independent of frame pins, and coalesces exact time/ordinal/count tombstones. Event append never deletes artifact or segment files and rolls back with `BudgetExhausted` when bounded event eviction cannot fit the batch.
- Startup runs artifact recovery first and then scans events in deterministic event-ID chunks. It repairs missing or inconsistent timeline/usage dependents, tombstones malformed payload/projection rows with valid scope/time, removes orphan event references, fails source-safely on unrecoverable identity/time corruption, and is idempotent. Session deletion cascades event rows/timeline/usage/tombstones and the existing deleted-session fence rejects late replay.

## Verification evidence

Rust 1.85 package verification ran in an isolated detached worktree based on `64e7f48` with only this checkpoint's cached patch applied, covering both affected crates:

- `cargo fmt --package krometrail-core --package krometrail-store -- --check` — passed.
- `cargo check -p krometrail-core -p krometrail-store --all-targets --locked` — passed.
- `cargo test -p krometrail-core -p krometrail-store --all-targets --locked` — passed; 88 core unit tests, 27 store unit tests, and every store integration target.
- `cargo clippy -p krometrail-core -p krometrail-store --all-targets --locked -- -D warnings` — passed.

Focused evidence covers fresh/v4/future/rollback migration behavior; atomic replay and conflict rollback; source-safe privacy/corruption handling; exact cursor/filter/priority/nearest ties; live-page/freelist classification; event-vs-segment ordering; pinned-frame survival; metadata-only append pressure; coalesced tombstones; complete session deletion; dependent/orphan repair; recoverable projection discard; fatal time corruption; and second-pass recovery idempotence.

No CDP, root composition, MCP, range-context service, parent-feature transition, or documentation change is included. The pre-existing `.work/bin/work-view` modification remains untouched and excluded.