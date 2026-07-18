---
id: epic-temporal-video-artifacts-ffmpeg-runtime
kind: feature
stage: drafting
tags: [infra, security, testing]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-clip-contracts]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Qualified FFmpeg runtime

## Brief

Deliver the optional production adapter for a user-installed `ffmpeg`: safe executable discovery, a bounded real MP4/H.264 qualification encode, exact implementation identity, and direct encoding through the injected clip contract. Process execution must use fixed allowlisted arguments without a shell and must enforce cancellation, deadline, output and diagnostic bounds, child termination/reaping, private temporary state, and atomic handoff of a validated result.

The adapter owns no download, installer, provider upload, MCP routing, retained artifact publication, or global FFmpeg configuration. Missing, unsuitable, changed, or vanished executables become safe qualification/runtime outcomes while the rest of Krometrail remains operational.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: infrastructure consumer of `epic-temporal-video-artifacts-clip-contracts`; can proceed in parallel with retained generation

## Simplification opportunity

- Keep executable discovery, qualification, sanitized diagnostics, and process-tree lifecycle in one direct Tokio adapter; avoid `ffmpeg-sidecar`, downloader features, native FFmpeg bindings, shell helpers, and duplicate subprocess abstractions unless concrete implementation evidence invalidates the simpler boundary.

## Foundation references

- `docs/SPEC.md` — Supported Environment and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Artifact Generation, Capability Registry, Failure Isolation, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip determinism and encoder provenance
- `docs/EVALUATION.md` — Temporal video generation, cancellation, and cleanup qualification

## Parent decisions inherited

- Qualification proves the produced MP4/H.264 contract rather than trusting names or version text.
- Only a versioned encoder/argument allowlist may be attempted, and the exact selected implementation is retained.
- Startup resolution creates one immutable availability/encoder identity until MCP restart.
- No bundled or managed FFmpeg and no UI surfaces apply.

