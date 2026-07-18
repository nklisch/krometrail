---
id: epic-temporal-video-artifacts-agent-surface-mcp-tool-and-resources
kind: story
stage: done
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

## Implementation notes

- Execution capability: GPT-5.6 Sol at xhigh reasoning, the caller-selected highest-capability fallback; one owner kept the registry, wire schema, retained-resource authority, and response projection aligned.
- Review weight: `standard` from the autopilot caller; this child advanced directly to `done` after green verification and receives no child-level review.
- Files changed: core temporal-video exports and schema proxy; MCP route registry, resource registry/reads, response projection, server composition guard, and hermetic fixtures/tests.
- Public surface: `generate_temporal_video` is registered only from the startup capability snapshot; its generated request schema is closed and reference-free with the two stable policies and fixed video output ceilings. Qualified startup also adds canonical local MP4 and manifest templates/reads; unavailable startup adds none of those video surfaces while existing still/frame surfaces remain unchanged.
- Compact response: ordered clips expose bounded identity, cache, media, hash, timing, selection counts, geometry, and canonical video/manifest URIs; each clip emits exactly two local resource links and no encoded bytes, provider fields, upload fields, or full provenance inline.
- Retained reads: canonical scoped video and manifest URIs dispatch through the injected `TemporalVideoGeneration` read authority with the shared deadline/cancellation boundary and fixed artifact byte limit; typed reads enforce identity, hash, length, media, and provenance invariants before MCP projection.
- Failure behavior: startup rejects both capability-without-service and service-without-capability construction. A fake post-start encoder loss returns stable `video_encoder_unavailable` recovery, and the same connection continues serving the unrelated tool registry.
- Tests added: qualified/unavailable registry and template symmetry; strict schema limits; canonical video URI parsing and unavailable read rejection; exact MP4/manifest resource reads; compact ordered multi-epoch response links; construction mismatch; request deadline/cancellation propagation; and post-start encoder failure isolation.
- Discrepancies from design: `McpDependencies` remains in the existing flat `config.rs` module. Policy schema coverage and core constructor tests cover both stable policy values; the MCP multi-epoch fake exercises `real_time` because response/resource semantics are policy-independent and are projected from typed manifests.
- Verification: `cargo fmt --all -- --check`; locked all-target workspace check; all 44 `krometrail-mcp` tests; MCP all-target Clippy with `-D warnings` — passed.
- Adjacent issues parked: none.
