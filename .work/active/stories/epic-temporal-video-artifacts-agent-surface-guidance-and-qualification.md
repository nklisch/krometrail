---
id: epic-temporal-video-artifacts-agent-surface-guidance-and-qualification
kind: story
stage: done
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

## Implementation notes

- Added still-first conditional-video guidance, local-resource and presentation-provenance rules,
  absent-tool recovery, restart semantics, and the explicit separation between encoder qualification
  and host/provider/model video support.
- Added optional typed F/G evaluation evidence while preserving A-E as the required canonical set.
- Added static distribution checks proving the plugin ships no FFmpeg asset or acquisition path, plus
  deterministic no-FFmpeg MCP coverage.
- Added an ignored opt-in live test that fails when its explicitly selected FFmpeg cannot qualify and,
  with the selected user installation, passed both policies through the real retained store, generation
  service, MCP tool, MP4 resource, and manifest resource.
- Validation passed: skill-creator `quick_validate.py` via isolated `uv`/PyYAML, plugin static checks,
  focused F/G and no-FFmpeg tests, live FFmpeg qualification, full workspace fmt/check/tests/Clippy,
  and the documentation build. The full gate also exposed and fixed a pre-existing environment-sensitive
  tool-count smoke by pinning its intended no-FFmpeg startup state.
