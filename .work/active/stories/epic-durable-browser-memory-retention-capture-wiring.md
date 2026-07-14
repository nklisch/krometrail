---
id: epic-durable-browser-memory-retention-capture-wiring
kind: story
stage: implementing
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
