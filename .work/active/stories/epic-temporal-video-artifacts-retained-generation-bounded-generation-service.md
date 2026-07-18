---
id: epic-temporal-video-artifacts-retained-generation-bounded-generation-service
kind: story
stage: done
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts-retained-generation
depends_on: [epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bounded retained temporal-video generation service

## Design checkpoint

Implement the resolved-range application service that partitions exact retained sources into visual epochs, derives bounded geometry and optional versioned meaningful-frame selection, builds the canonical plans, creates source/gap encoder inputs, serializes equal cache keys through one transient lock, invokes the injected encoder, rejects contradictory output, and publishes complete ordered clip results through the shared artifact store.

## Acceptance evidence

- Pure tests prove exact geometry fitting, deterministic visible meaningful-frame selection, gap-slate rendering, source/gap input ordering, and cache sensitivity.
- A deterministic fake encoder proves exact plan/profile/frame composition, cache reuse and equal-key single encoding, encoder identity/profile/hash rejection, multi-epoch all-or-error results, and no FFmpeg/process dependency.
- Cancellation/deadline tests cover source load, scheduler/lock wait, selection, encode, and pre-publication boundaries without publishing a partial clip.

## Ordering constraints

- Depends on `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence`.
- The lifecycle qualification checkpoint exercises this service against the real store and may tighten implementation, but may not introduce an alternate cache, range, plan, or deletion authority.

## Execution contract

- Worker capability: highest available, selected by active autopilot because exact cache/provenance and external-work cancellation are high consequence.
- Review weight: `standard` from autopilot default; this child closes on green evidence and the integrated feature receives the single independent review pass.

## Implementation notes

- Implemented by the highest-available Sol worker at `xhigh` reasoning; the integrated feature retains the orchestrator's `standard` review weight.
- Added the bounded application service over the injected `FrameSource`, shared `ArtifactStore`, `IdSource`, and `TemporalVideoEncoder` ports. It derives the caller/service deadline, isolates request/blocking/analysis permits, partitions exact retained sources, serializes equal cache keys with one weak transient lock, rejects contradictory encoder identity/profile output, and publishes only complete typed results.
- Added exact aspect-preserving no-upscale geometry fitting with explicit trailing one-pixel padding, streaming strict decode to bounded 256-pixel thumbnails for the versioned temporal-vision selector, deterministic visible meaningful-frame filtering, and deterministic patterned/labeled PNG gap slates. Real-time generation performs no visual decode.
- Added video cache identity over the canonical plan/profile/encoder/selector transcript and exact ordered source metadata/bytes while leaving the existing still-cache transcript and tests unchanged.
- Added the root composition hook that accepts an already-qualified encoder; this feature does not discover FFmpeg or conditionally wire an agent/MCP surface.
- Fake-port evidence covers exact plan/profile/frame composition, repeat hits, concurrent equal-key single encoding, model-selector determinism, encoder-identity contradiction rejection, cancellation/deadline before source I/O, cancellation during encode with cleanup, multi-epoch all-or-error behavior, geometry boundaries, deterministic valid gap PNGs, and video-cache sensitivity. No FFmpeg, browser, network, or provider is used.
- Verification: `cargo test --bin krometrail --locked` (116 passed, 2 intentionally ignored manual qualification/benchmark tests); `cargo check --workspace --all-targets --locked`; `cargo clippy --bin krometrail --tests --locked -- -D warnings`; focused eight-test service suite; focused video-cache test; `git diff --check`.
- Simplification pass: reused the existing epoch validator, strict decoder, pure video planner, source fingerprints, cancellation token, artifact port, and temporal-vision selector. No second persistent cache, range resolver, deletion registry, codec parser, or process layer was added.
- Discrepancy: the workspace-wide Clippy run currently stops in the independently implemented `krometrail-ffmpeg` crate on `clippy::items-after-test-module` in `process.rs`; retained-generation targets are clean and this story does not own that file. Parked work: none.
