---
id: epic-agent-browser-operation-mcp-control-surface
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-waits-and-batches]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# MCP Browser-Control Surface

## Brief

Expose the integrated control capability to coding agents over MCP stdio through composable lifecycle, page, observation, navigation, interaction, wait, evaluation, screenshot, and batch tools. Register only enabled capability tools, derive standalone and batch schemas from the shared Rust operation contracts, and return concise structured results with stable errors, interaction anchors, a context-sized post-action image when appropriate, and resource references for larger outputs.

Keep handlers thin: validate external input, invoke one application service, and map the domain result without embedding CDP commands, target logic, persistence, or image processing. This feature turns the reserved MCP crate into the agent-facing adapter and root-wires it; temporal investigation tools, durable artifact resources, and unavailable page/framework-state capabilities remain outside this epic.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: final consumer — exposes the completed browser-control operation set after waits and batching integrate both standalone families
- Inherited decisions: capability and action registries are single sources of truth; disabled capabilities contribute no tools; local stdio is the supported transport

## Simplification opportunity

- Generate schemas and registration from shared capability/action contracts and keep one response/error translator. Do not preserve the empty placeholder shape, add handwritten schema mirrors, or create an alternate CLI/daemon control runtime alongside MCP.

## Foundation references

- `docs/VISION.md` — Core Experience and Local-First Operation
- `docs/SPEC.md` — Browser-Control Surface, Capabilities, Current-State Observation, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Capability Registry, MCP Boundary, and Dependency Direction
- `docs/EVALUATION.md` — Browser-Control Evaluation

<!-- The feature-design pass will fill in interfaces, signatures, and implementation units. -->
