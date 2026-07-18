---
id: epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition
kind: story
stage: implementing
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts-agent-surface
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Runtime-qualified video availability and composition

## Design checkpoint

Add `temporal-video` to the capability registry as runtime-qualified, resolve one immutable startup snapshot from the bounded FFmpeg qualification result, and compose the retained generation service only when qualified. MCP startup must stay healthy when unavailable and emit one privacy-safe actionable diagnostic; availability changes only after restart.

## Acceptance evidence

- Core tests cover qualified, unavailable, disabled, dependency, explicit-selection, and deterministic snapshot ordering.
- Composition tests prove one qualification result controls both service injection and capability state, with mismatches rejected before serving.
- Missing/unsupported/unsuitable FFmpeg starts the existing MCP surface normally and logs one bounded safe reason; a qualified identity enables the service without leaking paths or page data.
- The operation and bounded scoped read contracts are additive, constructor-validated, and implemented by the existing retained-generation service/store authority.

## Ordering constraints

- Root checkpoint for this feature; both upstream feature dependencies are already implemented and approved.
- MCP registration must consume this snapshot and optional service rather than rediscovering FFmpeg.

## Execution contract

- Worker capability: highest available, selected by autopilot because this checkpoint joins the security-sensitive process authority to the stable capability surface.
- Review weight: `standard`; this child closes on green evidence and the integrated feature receives one independent review pass.
