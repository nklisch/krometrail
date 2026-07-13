---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Build the bounded capture engine

## Scope

Implement Unit 1 of the parent design: the core capture status/gap contracts and transport-neutral per-target screencast engine. This is the trickiest unit and must establish the exact receive → acknowledgement completion → bounded handoff contract before session lifecycle wiring uses it.

Use only the existing `CdpTransport` raw/named-event seam. For each `CaptureTarget`, subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before sending `Page.startScreencast`. On a frame, record `ObservedTime`, extract only the integer acknowledgement token, complete `Page.screencastFrameAck` under the configured timeout, then parse metadata and call `try_send` into a per-target bounded queue. Never wait for queue capacity.

The target worker owns base64 decode, JPEG/PNG header dimension reading, `EncodedFrame` construction, and `RecordingSink` calls. It performs no pixel decode/transcode or visual analysis. A bounded drop ledger remains independent of frame capacity so saturation, rejection, sequence loss, visibility, persistence rejection, and stop abandonment cannot disappear merely because the frame queue is full.

Implement the exact public contracts and signatures specified under the parent feature's “Trickiest unit” and “Unit 1” sections, including `CaptureConfig`, `CaptureDependencies`, `CaptureTarget`, `CaptureObserver`, `CaptureCoordinator`, `CaptureStreamState`, acknowledged capture statistics, `TargetCaptureStatus`, and `CaptureGapReason::FrameRejected`.

Keep target state keyed by `(TargetId, attachment_generation)`. Sequence comparison resets per generation. Preserve optional Chrome source time, daemon observed time, and normalized session time as independent values. Status/log fields follow the parent privacy allowlist.

Do not modify browser-session ports, production session supervision, root composition, real-Chrome tests, durable storage, temporal vision, or spike code.

## Required files

- `Cargo.toml`
- `Cargo.lock`
- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/capture/mod.rs` (new)
- `crates/krometrail-cdp/src/capture/pipeline.rs` (new)
- `crates/krometrail-cdp/tests/capture_pipeline.rs` (new)

Add workspace `base64` and `image` dependencies; disable `image` default features and enable only JPEG/PNG support. No other image or metrics dependency is needed.

## Acceptance criteria

- [ ] `CaptureStatistics` validates `acknowledged <= received`, `accepted + dropped <= acknowledged`, and `persisted <= accepted` with checked arithmetic; stable capture states and all gap reasons derive wire/display values from their single registries.
- [ ] `TargetCaptureStatus` validates non-zero queue capacity, depth not exceeding capacity, coherent frame/statistics state, generation, last-frame time, and acknowledgement measurements.
- [ ] A deterministic transport barrier proves no payload parse, queue attempt, image-header work, observer gap, or sink call occurs before ack completion.
- [ ] Ack failure/timeout hands nothing off, marks only that stream failed, and never increments accepted/dropped as though an ack succeeded.
- [ ] A blocked sink and tiny queue remain bounded; every post-ack full/closed handoff increments exactly one dropped path and yields explicit `IngestionQueueSaturated`/`CaptureStopped` evidence through the bounded ledger.
- [ ] Ledger capacity is fixed. Conservative coalescing retains exact estimated loss count and never implies continuity or allocates in proportion to dropped frames.
- [ ] Base64/header work happens only in the worker after acceptance. Empty, malformed, unsupported, or over-limit data emits `FrameRejected`; valid JPEG/PNG keeps encoded bytes unchanged and reports header dimensions.
- [ ] Sequence discontinuities emit exact estimated missing count only within one attachment generation; first frame of a generation has no comparison with the prior generation.
- [ ] Visibility false opens one hidden interval; true or an actual frame closes it; repeated signals coalesce and visible silence is never inferred as a gap.
- [ ] Source timestamps are checked/rounded independently, observed time is captured at return, session time derives only through the fixed `SessionOrigin`, and wall time is absent.
- [ ] Observer/status/log tests reject URL/title/browser key/CDP session/raw params/payload fields at info level.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and the production `--no-default-features` check pass with no session-wiring story required.

## Execution

- Effective worker: `highest`.
- Review weight: `standard` at the parent feature; story verification may fast-advance, while final feature review remains deeper.
- File ownership is exclusive to this story in the planned wave. The dependent wiring story edits different session/port/composition files.
