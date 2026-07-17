---
id: epic-agent-browser-reliability-viewport-emulation
kind: feature
stage: drafting
tags: [browser, agent-ux, visual]
parent: epic-agent-browser-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Target-scoped viewport emulation

## Brief

Resolve GitHub issue #10 with an additive browser-control operation that applies or clears explicit viewport/device metrics on one selected target and reports the effective CSS viewport, device scale, mobile layout, and touch state. The override must survive ordinary navigation, be restored or explicitly cleared across target attachment lifecycle, and avoid opaque named-device presets in the first stable contract.

Viewport changes during recording must remain honest in source-frame metadata and artifact normalization. The operation returns live evidence under the same outcome rules as other state-changing controls and records a correlation marker suitable for later temporal analysis.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: independent public capability consumed by final MCP schema and skill guidance.

## Simplification opportunity
- Establish one explicit viewport authority rather than adding both launch-only sizing and a separate runtime preset system.

## Foundation references
- `docs/SPEC.md` — viewport/output and browser-control contracts
- `docs/ARCHITECTURE.md` — target-scoped control state and reconnect restoration
- `docs/VISUAL-EVIDENCE.md` — frame geometry and normalization
