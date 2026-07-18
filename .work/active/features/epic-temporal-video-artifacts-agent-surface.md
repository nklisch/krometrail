---
id: epic-temporal-video-artifacts-agent-surface
kind: feature
stage: drafting
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-ffmpeg-runtime, epic-temporal-video-artifacts-retained-generation]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Conditional temporal video agent surface

## Brief

Join the qualified encoder and retained generation service at the composition root, project one startup availability snapshot through the capability registry, and register the temporal-video MCP tool and video resources only when qualification succeeds. The public request exposes both bounded policies through generated validated schemas; responses return the local `video/mp4` resource and manifest, while a post-start encoder loss maps to a stable actionable error without affecting still artifacts or capture.

Update the shipped Krometrail skill and progressive evidence guidance so agents can explain an absent tool, tell a user how their own FFmpeg installation enables it after restart, and recommend video only when the host/model is already known to accept it. This feature also owns registry/schema/resource/plugin contract coverage and opt-in end-to-end qualification, but it does not add provider uploads, automatic model-capability detection, a product-managed encoder, or a human UI.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: integration and consumer feature — depends on both the qualified runtime and retained-generation branches

## Simplification opportunity

- Make registry-owned runtime availability the single authority for tool discovery, service injection, diagnostics, schemas, resources, and skill wording; remove the need for a dead placeholder tool or scattered FFmpeg checks in handlers and prose.

## Foundation references

- `docs/VISION.md` — Core Experience and Visual Evidence
- `docs/SPEC.md` — Capabilities, Temporal Queries, Errors and Degraded Operation, and Local Data
- `docs/ARCHITECTURE.md` — Capability Registry, MCP Boundary, Observability, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip and Progressive Detail
- `docs/EVALUATION.md` — Optional video conditions and Temporal video evaluation
- `plugin/skills/krometrail/SKILL.md` — installed agent workflow and capability discoverability

## Parent decisions inherited

- Conditional registration is based on one bounded startup qualification snapshot and changes only after MCP restart.
- Krometrail does not infer model video support; skill recommendations are conditional on known host/model capability.
- The runtime returns local provider-neutral resources and never uploads or attaches them.
- No UI surfaces or mockups apply.

