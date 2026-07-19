---
id: epic-agent-surface-simplification-persistence-recovery
kind: feature
stage: drafting
tags: [browser, storage, diagnostics]
parent: epic-agent-surface-simplification
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Recoverable segment publication and actionable capture failures

## Brief

Repair the reproducible capture failure at the 120-second segment-rotation boundary. A completed sealed-file rename followed by directory-sync failure currently becomes a permanently latched writer error, poisoning every later browser session in the MCP process. Classify that publication failure as recoverable when writer state is known, retain terminal latching for ambiguous partial writes, and prove the next append can proceed safely.

Carry the first privacy-safe persistence operation/category through capture status, diagnostics, and structured shutdown recovery. Replace the bare degraded stop outcome with enough closure quality, failed phase, capture cause, and recovery guidance for an agent to distinguish restart-required terminal state from successfully closed degraded evidence.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: independent recording reliability and diagnostic feature

## Simplification opportunity

Use the store's already-safe operation plus `ErrorKind` classification as one inward failure contract. Delete generic error discards and bare degraded enums rather than adding a second diagnostic side channel.

## Foundation references

- `docs/SPEC.md` — Continuous Visual Capture and Failure Semantics
- `docs/ARCHITECTURE.md` — Segment Format, Crash Recovery, Diagnostics and Observability
