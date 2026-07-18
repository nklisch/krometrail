---
id: epic-temporal-video-artifacts-agent-surface-guidance-and-qualification
kind: story
stage: pending
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts-agent-surface
depends_on: [epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition, epic-temporal-video-artifacts-agent-surface-mcp-tool-and-resources]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Video guidance and end-to-end qualification

## Design checkpoint

Update the shipped skill, setup/evidence references, plugin/catalog assertions, and optional evaluation vocabulary to describe the implemented conditional surface truthfully. Add hermetic contract coverage plus an explicitly invoked real user-FFmpeg qualification of both policies and local MP4/manifest reads, without model calls, uploads, downloads, or bundled encoder assets.

## Acceptance evidence

- Skill tests prove still-first guidance, absent-tool diagnosis, user-installed FFmpeg/path enablement, restart requirement, local-resource use, presentation-hold provenance, and the distinction between encoder availability and known host/model support.
- Plugin/install/release checks prove no FFmpeg binary or acquisition path is shipped and a no-FFmpeg startup preserves the existing tool surface while omitting video.
- Evaluation tests retain A-E as required and model real-time/model-optimized video as optional conditions bound to exact host/provider/model, encoder, policy, output hash, resource, and manifest evidence.
- Hermetic fakes require no FFmpeg/network. The opt-in live test, once explicitly invoked, fails rather than skips on qualification failure and validates both policies through the real store/service/MCP resource path.

## Ordering constraints

- Depends on both runtime composition and the complete MCP tool/resource checkpoint.
- This checkpoint documents and evaluates the implemented contract; it must not infer model support, upload evidence, add provider adapters, or manage FFmpeg installation.

## Execution contract

- Worker capability: highest available, selected by autopilot because shipped agent guidance and evaluation claims must match stable runtime discovery exactly.
- Review weight: `standard`; this child closes on green evidence and the integrated feature receives one independent review pass.
