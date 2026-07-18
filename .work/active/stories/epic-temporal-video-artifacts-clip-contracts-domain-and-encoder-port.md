---
id: epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port
kind: story
stage: implementing
tags: [visual, agent-ux, security]
parent: epic-temporal-video-artifacts-clip-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Validated clip domain and encoder port

## Design checkpoint

Establish the constructor-validated one-epoch video-plan values, conservative server ceilings, fixed silent MP4/H.264 profile, privacy-safe encoder identity, encoded-frame request/result values, object-safe encoder port, and stable encoder error codes described in the parent feature. This checkpoint supplies the shared types required by both the pure planner and the later FFmpeg/service branches; it does not implement timing policy or any external process.

## Acceptance evidence

- `krometrail-core` tests prove policy/timing/error stable names, constructor-backed Serde/schema rejection, one-epoch/frame/geometry/limit invariants, exact encode segment inputs, output hashes, and object-safe fake-port use.
- The public core/ports exports compile without adding a Tokio process, filesystem, FFmpeg, MCP, store, or provider dependency.
- Invalid identities cannot persist paths/control characters or bypass the closed media/no-audio contract.

## Ordering constraints

- Root checkpoint for this feature.
- The presentation planner and manifest checkpoints consume these exact values and must not introduce alternate plan or encoder shapes.
