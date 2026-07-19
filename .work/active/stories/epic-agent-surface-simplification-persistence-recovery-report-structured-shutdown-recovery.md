---
id: epic-agent-surface-simplification-persistence-recovery-report-structured-shutdown-recovery
kind: story
stage: implementing
tags: [browser, agent-ux, diagnostics]
parent: epic-agent-surface-simplification-persistence-recovery
depends_on: [epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Report structured closure quality, cause, and recovery

Replace the scalar degraded stop outcome with a structured closure result that separates released browser authority from evidence quality. Carry the first capture cause, typed failed phase, and recoverability-specific action through shutdown, concise status, MCP warnings, schemas, and current foundation docs.

## Acceptance evidence

- Clean managed and attached stops serialize as structured clean outcomes.
- A successfully closed browser with failed capture flush returns a degraded tool envelope with exact phase, capture cause, and recovery.
- `writer_usable` directs a new browser session; `writer_terminal` directs an MCP restart before a new session.
- Remaining managed process/profile authority still fails with `shutdown_incomplete`.
- End-to-end regression proves a fresh session persists after the post-rename sync failure in the same MCP process.

## Ordering

Depends on first-cause capture propagation and completes the externally visible recovery contract.
