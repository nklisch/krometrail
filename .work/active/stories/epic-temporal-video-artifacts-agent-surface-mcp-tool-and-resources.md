---
id: epic-temporal-video-artifacts-agent-surface-mcp-tool-and-resources
kind: story
stage: implementing
tags: [agent-ux, infra, testing]
parent: epic-temporal-video-artifacts-agent-surface
depends_on: [epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Conditional temporal-video MCP tool and resources

## Design checkpoint

Project the immutable capability snapshot into one registry-derived `generate_temporal_video` route, strict generated schema, compact response, and conditional MP4/manifest resource templates and reads. Qualified startup exposes the complete surface; unavailable startup exposes none of it while preserving every existing still/frame tool and resource.

## Acceptance evidence

- Registry tests compare qualified and unavailable configurations and prove tool, schema, templates, and reads agree with the same snapshot.
- Fake-service calls cover both policies, multi-epoch ordered clips, request budget/cancellation, compact structured output, two local resource links per clip, and no inline video/provider/upload fields.
- Scoped MP4 and manifest resource reads validate canonical URI, identity, media, hash, length, and byte limit through the retained service authority.
- A fake after-start encoder loss returns stable `video_encoder_unavailable` recovery while unrelated routes continue to work; configuration/service mismatches fail construction.

## Ordering constraints

- Depends on `epic-temporal-video-artifacts-agent-surface-runtime-availability-and-composition`.
- Guidance and live qualification depend on this exact public discovery/resource behavior and may not create alternate names or availability checks.

## Execution contract

- Worker capability: highest available, selected by autopilot because generated stable MCP contracts and retained local resources are externally observable 1.x boundaries.
- Review weight: `standard`; this child closes on green evidence and the integrated feature receives one independent review pass.
