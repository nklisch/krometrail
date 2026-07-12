---
id: epic-agent-browser-operation
kind: epic
stage: drafting
tags: [browser, agent-ux]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
---

# Agent Browser Operation

## Brief

This epic gives coding agents the complete live browser workflow that temporal inspection extends. It delivers current structured page observations, generation-scoped actionable references, verified browser input, navigation and tab management, explicit waits, batches, page evaluation, and post-action screenshots.

The control surface follows contemporary browser-agent conventions while preserving Krometrail’s local-first and fail-fast posture. Every state-changing standalone action produces a live observation and an interaction anchor; stale references and silent no-ops become explicit failures rather than guessed successes.

This epic does not provide historical temporal bundles or derived visual artifacts. It establishes reliable ordinary browser use and the interaction records that later temporal queries reference.

## Foundation references

- `docs/VISION.md` — Product Thesis and Core Experience
- `docs/SPEC.md` — Current-State Observation, Structured Page Snapshots, Browser-Control Surface, and Capabilities
- `docs/ARCHITECTURE.md` — Structured Snapshots and References, Interaction Execution, Capability Registry, and MCP Boundary
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Anticipated child features

- Accessibility snapshots and generation-scoped actionable references
- Page and history navigation plus target selection and lifecycle actions
- Pointer, keyboard, form, scroll, drag, dialog, and upload interactions
- Action-specific completion and wait semantics
- Post-action screenshots and structured live observations
- Ordered action batches and interaction anchors
- Capability-driven Rust MCP tools, resources, and generated schemas

<!-- The design pass on each child feature will fill in real specifics. -->
