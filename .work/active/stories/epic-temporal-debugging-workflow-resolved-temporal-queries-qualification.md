---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-qualification
kind: story
stage: implementing
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
