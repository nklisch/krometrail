---
id: epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification
kind: story
stage: done
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

## Implementation notes

- Execution capability: GPT-5.6 Sol xhigh, the caller-selected Luna fallback for the external executable, privacy, and produced-contract qualification risk.
- Review weight: `standard` from the autopilot caller; this child checkpoint advances directly to `done`, with review reserved for the integrated feature.
- Files changed: `discovery.rs`, `qualification.rs`, the minimal immutable qualified encoder holder, crate exports, bounded output read support, and the compiled fixture/integration qualification suite.
- Tests added: invalid-explicit no-fallback; canonical PATH deduplication and candidate ceiling; executable digest/stamp drift; exact build-report framing and safe version labels; valid produced-contract qualification; version-only invalid output; unsupported encoder exit; oversized version report; and snapshotted PATH discovery. These protect discovery precedence, identity privacy, and the real encode gate.
- Simplification: discovery is an internal bounded path walk with no `which`/`where`, recursive search, environment reread, generic codec negotiation, or separate probe encoder path.
- Discrepancies from design: added `with_search_path` as a narrow unpublished options constructor so composition and hermetic tests can inject an already-snapshotted PATH without mutating process environment; qualification semantics are unchanged.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-ffmpeg` passed 25 deterministic tests and doc tests without invoking a real FFmpeg executable or network.
