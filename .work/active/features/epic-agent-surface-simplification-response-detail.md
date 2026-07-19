---
id: epic-agent-surface-simplification-response-detail
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-current-contract]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Concise, expanded, and full agent responses

## Brief

Replace the public response projection matrix with one detail progression: implicit `concise`, explicit `expanded`, and explicit `full`. Concise browser actions expose outcome, page/navigation/focus changes, warnings, anchors, resources, and a bounded flattened exact-target index. Expanded adds bounded semantic/page context; full returns complete acquired structures. Inline image transport remains an orthogonal opt-in.

Delete legacy, compact, interaction-only, omit, and public diagnostic-suppression variants. Failed and degraded results always expose privacy-bounded diagnostics. Update generated schemas, registry routing, skill instructions, documentation, and protocol tests to teach omission-first routine use and deliberate expansion.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: agent-facing contract consumed by batch and temporal economy features

## Simplification opportunity

Delete per-part preference enums and switches, test-only legacy bundles, projection-omitted markers, ancestor-closure reconstruction, duplicate server parsing for diagnostic preferences, and obsolete projection tests. Keep one canonical-result projection path and small MCP-specific concise output types.

## Foundation references

- `docs/VISION.md` — Core Experience
- `docs/SPEC.md` — Current-State Observation and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — MCP Boundary
