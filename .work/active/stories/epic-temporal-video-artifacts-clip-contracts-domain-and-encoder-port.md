---
id: epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port
kind: story
stage: done
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

## Implementation notes

- Execution capability: GPT-5.6 Sol at xhigh, selected by the active autopilot caller because this checkpoint defines stable, security-sensitive contracts consumed by two downstream features.
- Review weight: `standard` (autopilot default); child stories close on green verification and the feature receives the independent review.
- Files changed: `crates/krometrail-core/src/video/{mod.rs,plan.rs,tests.rs}`, `crates/krometrail-core/src/ports/video.rs`, core port/lib exports, `error.rs`, and the additive `JsonSchema` derive on the reused `VisualEpoch`.
- Tests added: constructor-backed policy/timing/geometry/plan round trips; cross-scope, epoch, order, and exact count boundaries; privacy-safe encoder identity; exact input/output byte boundaries; request segment matching; output hashing; object-safe fake encoder use; and stable error retry/recovery behavior.
- Simplification: reused `CapturedFrame`, `ResolvedRange`, `VisualEpoch`, `PixelDimensions`, `ImageFormat`, `CancellationSignal`, `PortFuture`, and `temporal_vision::OutputHash`; the port remains runtime/process/filesystem neutral and defines one closed media profile.
- Discrepancies from design: the repository's `ImageFormat` is already a closed JPEG/PNG enum, so unsupported image formats are rejected by construction/Serde rather than a redundant request branch; the existing public-field `VisualEpoch` remains unchanged and is revalidated at `VideoPlanInput` and plan construction boundaries.
- Verification: `cargo fmt --all -- --check`, `cargo clippy -p krometrail-core --all-targets -- -D warnings`, and `cargo test -p krometrail-core --all-targets` passed (127 tests).
- Adjacent issues parked: none.
