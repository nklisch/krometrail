---
id: epic-agent-browser-reliability-managed-session-lifecycle
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Reliable managed-browser lifecycle

## Brief

Correct GitHub issues #3, #4, and #5 across discovery, foreground interaction, and shutdown. Standard macOS Chrome discovery must tolerate a cold application version probe and report attempted candidates safely. Pointer operations against a hidden managed target must either recover through Krometrail-owned activation or return a specific actionable visibility failure rather than a generic observation error.

Shutdown results must describe remaining cleanup, not historical capture health: once the managed process/session/profile authority is released enough for an immediate restart, stop succeeds or returns an explicitly degraded result; a true incomplete result identifies the remaining resource safely.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: consumes durable diagnostic correlation; independent of capture outcome and input semantics implementation.

## Simplification opportunity
- Use the existing target activation and process authority instead of documenting external macOS automation as recovery.

## Foundation references
- `docs/SPEC.md` — managed lifecycle and stable error behavior
- `docs/ARCHITECTURE.md` — launcher, target supervisor, and shutdown ownership
