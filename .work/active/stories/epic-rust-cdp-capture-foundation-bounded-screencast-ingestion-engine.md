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

Use only the existing `CdpTransport` raw/named-event seam. Gate the whole capture module behind the default `cdpkit-transport` feature. For each exact `CaptureTarget` context, subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before sending `Page.startScreencast`. On a frame, record `ObservedTime`, extract only integer `params.sessionId`—the official CDP frame number and acknowledgement token—complete `Page.screencastFrameAck` under the configured timeout, record ack latency, then parse metadata and call `try_send` into a per-target bounded queue. Never wait for queue capacity.

The target worker owns base64 decode, a local bounded JPEG SOF/PNG IHDR dimension parser, `EncodedFrame` construction, and `RecordingSink` calls. It performs no pixel decode/transcode or visual analysis and adds no general image dependency. JPEG marker walking uses checked lengths and stops after 64 KiB; PNG accepts only a valid signature/fixed IHDR width and height. A bounded drop ledger remains independent of frame capacity so saturation, rejection, frame-number discontinuity, visibility, persistence rejection, and stop abandonment cannot disappear merely because the frame queue is full.

Implement the contracts and visibility specified under the parent feature's “Trickiest unit” and “Unit 1” sections. Only `CaptureConfig` is public for root composition. `CaptureDependencies`, `CaptureTarget`, `CaptureObserver`, `CaptureCoordinator`, `CaptureError`, `CaptureStopReason`, `CaptureStopOutcome`, and `CaptureShutdownOutcome` are crate-private and not re-exported. Core `CaptureStreamState`, acknowledged capture statistics, `CaptureTimingSummary`, `TargetCaptureStatus`, and `CaptureGapReason::FrameRejected` remain adapter-neutral public contracts.

Keep target state keyed by `(TargetId, attachment_generation)` and carry exact connection generation/transport session in the target context. Preserve the official frame number as `source_sequence`; comparison resets per generation. The constant scripted candidate fixture is not evidence against this protocol contract, and this story does not claim to prove live numbering. Preserve optional Chrome source time, daemon observed time, and normalized session time independently. `SessionOrigin` must have been sampled before any subscriptions/start; observed/session values are nondecreasing (`>=`) so equal monotonic readings are valid. Status/log fields follow the parent privacy allowlist.

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
- `crates/krometrail-cdp/src/capture/image_header.rs` (new)
- `crates/krometrail-cdp/src/capture/tests.rs` (new)

Add only workspace `base64`. Activate capture-only Tokio/base64 requirements from `cdpkit-transport`; do not add `image` or another metrics dependency. Keep tests inside `src/capture/tests.rs` so crate-private engine types do not become public for integration-test convenience.

## Acceptance criteria

- [ ] `CaptureStatistics` validates `acknowledged <= received`, `accepted + dropped <= acknowledged`, and `persisted <= accepted` with checked arithmetic; stable capture states and all gap reasons derive wire/display values from their single registries.
- [ ] `TargetCaptureStatus` validates non-zero queue capacity, depth not exceeding capacity, coherent frame/statistics state, generation, last-frame time, and acknowledgement measurements.
- [ ] A deterministic transport barrier proves no payload parse, queue attempt, image-header work, observer gap, or sink call occurs before ack completion. With a permanently blocked sink and saturated queue, ack completion and histogram recording continue on the same pre-handoff path and do not inspect/wait for queue occupancy; this is a structural proof, not a fragile latency threshold.
- [ ] Ack failure/timeout hands nothing off, marks only that stream failed, and never increments accepted/dropped as though an ack succeeded.
- [ ] A blocked sink and tiny queue remain bounded; every post-ack full/closed handoff increments exactly one dropped path and yields explicit `IngestionQueueSaturated`/`CaptureStopped` evidence through the bounded ledger.
- [ ] Ledger capacity is fixed. Conservative coalescing retains exact estimated loss count and never implies continuity or allocates in proportion to dropped frames.
- [ ] Base64/header work happens only in the worker after acceptance. Empty, malformed, unsupported, over-limit, missing-IHDR, or no-SOF-within-64-KiB data emits `FrameRejected`; valid JPEG/PNG keeps encoded bytes unchanged and reports header dimensions without `image` or pixel decoding.
- [ ] Official CDP frame numbers are preserved as `source_sequence`; discontinuities emit exact estimated missing count only within one attachment generation, and the first frame of a generation has no comparison with the prior generation. Scripted constant values are labeled fixture behavior, while live increasing evidence is deferred to the real-Chrome story.
- [ ] Visibility false opens one hidden interval; true or an actual frame closes it; repeated signals coalesce and visible silence is never inferred as a gap.
- [ ] `SessionOrigin` happens-before subscriptions/start/first frame. Source timestamps are checked/rounded independently, observed time is captured at return, session time derives only through the fixed origin, observed/session ordering is nondecreasing (`next >= previous`), equal samples are accepted, and wall time is absent.
- [ ] The parent `CaptureStreamState` transition table is implemented exhaustively; terminal/invalid transitions cannot restart a stream and observer state events are transition-only.
- [ ] Fixed 64-bucket logarithmic ack-latency and inter-frame-cadence histograms remain constant-memory, accept zero-duration samples, and expose deterministic sample-count/nearest-rank p50/p95/p99 bucket bounds plus exact max.
- [ ] Defaults (8 active streams × 4 slots × 8 MiB base64 text) and every accepted override stay within the 256 MiB queued-payload ceiling. Checked arithmetic rejects overflow and combinations beyond the ceiling; hard caps are 32/16/16 MiB. Ledgers/histograms are separately fixed-size.
- [ ] `CaptureConfig` is the only exported capture type; coordinator/dependencies/target/observer/error/stop/outcomes are crate-private and covered by internal tests.
- [ ] Observer/status/log tests reject URL/title/browser key/CDP session/raw params/payload fields at info level.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass with no session-wiring story required.

## Execution

- Effective worker: `highest`.
- Review weight: `standard` at the parent feature; story verification may fast-advance, while final feature review remains deeper.
- File ownership is exclusive to this story in the planned wave. It does not add `BrowserSessionEvent` variants or edit target reducer/model files; the dependent wiring story owns those changes, preserving a compile-real boundary.
