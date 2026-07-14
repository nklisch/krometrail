---
id: epic-durable-browser-memory-retention-capture-wiring
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: [epic-durable-browser-memory-retention-removal-engine]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Paused-Budget Capture and Root Composition

## Checkpoint

Wire the shared `RetentionStore` into CDP capture/status and root composition. Handle only `BudgetExhausted` as a recoverable `PausedBudget` transition, keep frame acknowledgement immediate, record bounded loss explicitly, wait without lost wakeups, resume after real space recovery, and make shutdown cancel a budget wait. Validate the global budget environment input and default to 10 GB.

## Ordering

Depends on the working removal engine because capture must wait on the production availability generation rather than a test-only flag.

## Acceptance evidence

- Browser status composes live retention and existing target capture evidence.
- Budget exhaustion pauses rather than fails, never deletes a pin, acknowledges subsequent CDP frames promptly, and records every known drop/gap.
- Unpin/deletion wakes capture only after enforcement reports availability; hidden/visible state resumes coherently.
- Stop while paused completes within the existing shutdown deadline.
- Non-budget persistence failures remain terminal.
- Root opens/resumes one recording store and shares it as recording+retention while `SqliteIndex` retains focused timeline/catalog/gap/frame ports.

## Implementation notes

- Execution capability: highest, caller-selected; this checkpoint crosses live CDP acknowledgement, async cancellation, durable retention state, and root composition.
- Review weight: standard (caller-selected); review remains at the parent feature boundary.
- Added a shared retention dependency to production capture. `BudgetExhausted` now emits the explicit persistence gap, enters `PausedBudget`, continues bounded acknowledgement/handoff, and resumes to the latest visible/hidden state only after the retention generation reports availability.
- A stop notification is armed without a lost-wakeup window, so a paused worker drains or abandons within the existing shutdown deadline. Non-budget persistence failures retain the terminal path.
- Browser status now reads the live retention store asynchronously without holding the supervisor lock. Root validates `KROMETRAIL_DISK_BUDGET_BYTES`, runs payload recovery before opening the journal-resuming `RecordingStore`, and shares that one store through recording and retention ports while focused SQLite query ports remain separate.
- Tests added: deterministic pause/ack/queue-loss/gap/resume/hidden-state coverage, paused cancellation, stable state-registry coverage, and source-safe budget configuration parsing.
- Verification: `cargo check --workspace --all-targets --locked`, 86 CDP library tests, and 5 root binary tests passed.
- Simplification: removed the stale root `IndexedRecordingSink` composition rather than adding a compatibility wrapper.
- Discrepancies from design: none.
- Adjacent issues parked: none.
