---
id: epic-temporal-video-artifacts-ffmpeg-runtime-encoder-port-adapter
kind: story
stage: done
tags: [infra, security, testing]
parent: epic-temporal-video-artifacts-ffmpeg-runtime
depends_on: [epic-temporal-video-artifacts-ffmpeg-runtime-managed-process-and-mp4-validation, epic-temporal-video-artifacts-ffmpeg-runtime-discovery-and-qualification]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Qualified temporal-video encoder adapter

## Design checkpoint

Implement `TemporalVideoEncoder` on the immutable qualified adapter, including one-process admission control, queue and active cancellation/deadline handling, executable-drift failure, stable core error mapping, validated result construction, hermetic process fixtures, and the explicit opt-in live FFmpeg qualification lane.

## Acceptance evidence

- The concrete adapter works behind `Arc<dyn TemporalVideoEncoder>` and returns the committed identity/profile/hash/bytes contract through core constructors.
- Cancellation and deadline tests cover both permit wait and an active process; dropped futures and all failures leave no process tree or partial caller-visible bytes.
- Stable mappings distinguish cancellation, resource overflow, executable unavailability, and sanitized encoding failure without exposing adapter causes.
- Default tests require no FFmpeg/network; the ignored explicit live test fails when the selected executable cannot produce the fixed playable contract and reports only safe identity evidence.

## Ordering constraints

- Depends on both the managed process/validator and discovery/qualification checkpoints.
- The later agent-surface feature owns composition and conditional registration; this checkpoint exports the qualified concrete adapter but does not register tools or publish retained artifacts.

## Implementation notes

- Execution capability: GPT-5.6 Sol xhigh, the caller-selected Luna fallback for the external-process, cancellation, and privacy-sensitive adapter boundary.
- Review weight: `standard` from the autopilot caller; this child checkpoint advances directly to `done`, with review reserved for the integrated feature.
- Files changed: the immutable qualified encoder trait adapter, cumulative microsecond PTS policy/job generation, request-time executable revalidation, managed-process test placement, compiled fixture observations, hermetic adapter integration tests, and the feature-gated explicit live qualification test.
- Tests added: object-safe `Arc<dyn TemporalVideoEncoder>` success and core result construction; sanitized stable error mapping; executable drift before staging; permit-wait and active cancellation/deadline behavior; aborted-future descendant cleanup; and explicit selected-real-FFmpeg qualification plus request encoding. These protect the public port boundary, process lifetime, private workspace cleanup, and produced-output contract.
- Simplification: one semaphore and the already-qualified executable feed the existing production encode/validation path; there is no per-request rediscovery, alternate live-test encoder, caller-visible staging path, or adapter-cause leakage.
- Discrepancies from design: the opt-in FFmpeg 8.0.1 lane exposed concat image-demuxer 25 fps timestamp quantization, so the fixed policy now assigns validated cumulative presentation PTS at a 1 MHz input/encoder/track timebase. Safe concat mode remains enabled, the terminal duplicate is placed at duration minus one microsecond, and the checked validator proved the exact 350 ms contract.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-ffmpeg` passed 32 deterministic tests and doc tests without invoking FFmpeg or network; `cargo clippy -p krometrail-ffmpeg --all-targets --all-features -- -D warnings` passed; the ignored explicit live lane qualified `/opt/homebrew/bin/ffmpeg` 8.0.1 with `libx264` and produced a validated 1,508-byte 350 ms H.264 MP4 with SHA-256 `8b3905f2acd80fc1f4c2a476e8339ca0c17d79c0efa668ba0f6894f6fad2c762`.
