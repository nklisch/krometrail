---
id: epic-temporal-video-artifacts-ffmpeg-runtime-encoder-port-adapter
kind: story
stage: implementing
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
