---
id: epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification
kind: story
stage: implementing
tags: [infra, security, testing]
parent: epic-temporal-video-artifacts-ffmpeg-runtime
depends_on: [epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Deterministic FFmpeg discovery and real qualification

## Design checkpoint

Implement bounded explicit/PATH/platform-default discovery, private executable identity pinning, bounded version-report hashing, safe identity derivation, and the tiny real encode probe that qualifies only the fixed produced MP4/H.264 contract. Missing or unsuitable installations remain a typed safe unavailable outcome.

## Acceptance evidence

- Hermetic discovery cases prove explicit override precedence, invalid-explicit no-fallback behavior, bounded canonical candidate deduplication, platform naming, and path-free failures.
- Qualification cases prove version text alone never enables the adapter and that the tiny probe uses the checkpointed production runner and MP4 validator.
- Identity cases prove build report, selected `libx264`, adapter version, and argument-policy version are exact and privacy-safe.
- Executable removal or ordinary replacement after qualification is detected without mutating the startup qualification snapshot.

## Ordering constraints

- Depends on `epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation`.
- The final encoder checkpoint consumes the exact qualified executable and identity; it may not rediscover or renegotiate FFmpeg per request.
