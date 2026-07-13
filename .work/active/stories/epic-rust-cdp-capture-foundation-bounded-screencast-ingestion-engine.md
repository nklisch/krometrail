---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine
kind: story
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Build the bounded capture engine

## Post-completion correction (2026-07-13)

This story is `done` because its implementation and review occurred, but part of its accepted contract was later disproved by production Chrome and canonical final5 evidence. The acknowledgement-first engine, bounded queue/ledger/histograms, three clocks, image-header handling, state machine, and target isolation remain valid. The claims that the CDP acknowledgement token is a usable source sequence, that it can expose discontinuities, and that it belongs in persisted frame metadata are superseded. `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-contract-remediation` owns their removal and the Krometrail `CaptureOrdinal` replacement. This historical story is not sufficient completion evidence for the revised feature.

## Scope

Implement Unit 1 of the parent design: the core capture status/gap contracts and transport-neutral per-target screencast engine. This is the trickiest unit and must establish the exact receive → acknowledgement completion → bounded handoff contract before session lifecycle wiring uses it.

Use only the existing `CdpTransport` raw/named-event seam. Gate the whole capture module behind the default `cdpkit-transport` feature. For each exact `CaptureTarget` context, subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before sending `Page.startScreencast`. On a frame, record `ObservedTime`, extract integer `params.sessionId`, complete `Page.screencastFrameAck` under the configured timeout, record ack latency, then parse metadata and call `try_send` into a per-target bounded queue. Never wait for queue capacity. **Correction:** implementation treated that integer as both acknowledgement token and source sequence; real Chrome shows it is ack-only, and the remediation story removes the second meaning.

The target worker owns base64 decode, a local bounded JPEG SOF/PNG IHDR dimension parser, `EncodedFrame` construction, and `RecordingSink` calls. It performs no pixel decode/transcode or visual analysis and adds no general image dependency. JPEG marker walking uses checked lengths and stops after 64 KiB; PNG accepts only a valid signature/fixed IHDR width and height. A bounded drop ledger remains independent of frame capacity so saturation, rejection, visibility, persistence rejection, and stop abandonment cannot disappear merely because the frame queue is full. The implemented frame-number-discontinuity branch is invalid and is removed by remediation rather than preserved as loss evidence.

Implement the contracts and visibility specified under the parent feature's “Trickiest unit” and “Unit 1” sections. Only `CaptureConfig` is public for root composition. `CaptureDependencies`, `CaptureTarget`, `CaptureObserver`, `CaptureCoordinator`, `CaptureError`, `CaptureStopReason`, `CaptureStopOutcome`, and `CaptureShutdownOutcome` are crate-private and not re-exported. Core `CaptureStreamState`, acknowledged capture statistics, `CaptureTimingSummary`, `TargetCaptureStatus`, and `CaptureGapReason::FrameRejected` remain adapter-neutral public contracts.

Keep target state keyed by `(TargetId, attachment_generation)` and carry exact connection generation/transport session in the target context. **Superseded premise:** the implementation preserved the acknowledgement token as `source_sequence` and compared it within a generation. Canonical final5 Linux/macOS traces and production Chrome 149 invalidate that premise. Remediation keeps the token only for acknowledgement and introduces a Krometrail-owned per-target-session `CaptureOrdinal` that continues across generations. Preserve optional Chrome source time, daemon observed time, and normalized session time independently. `SessionOrigin` must have been sampled before any subscriptions/start; observed/session values are nondecreasing (`>=`) so equal monotonic readings are valid. Status/log fields follow the parent privacy allowlist.

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

- [x] `CaptureStatistics` validates `acknowledged <= received`, `accepted + dropped <= acknowledged`, and `persisted <= accepted` with checked arithmetic; stable capture states and all gap reasons derive wire/display values from their single registries.
- [x] `TargetCaptureStatus` validates non-zero queue capacity, depth not exceeding capacity, coherent frame/statistics state, generation, last-frame time, and acknowledgement measurements.
- [x] A deterministic transport barrier proves no payload parse, queue attempt, image-header work, observer gap, or sink call occurs before ack completion. With a permanently blocked sink and saturated queue, ack completion and histogram recording continue on the same pre-handoff path and do not inspect/wait for queue occupancy; this is a structural proof, not a fragile latency threshold.
- [x] Ack failure/timeout hands nothing off, marks only that stream failed, and never increments accepted/dropped as though an ack succeeded.
- [x] A blocked sink and tiny queue remain bounded; every post-ack full/closed handoff increments exactly one dropped path and yields explicit `IngestionQueueSaturated`/`CaptureStopped` evidence through the bounded ledger.
- [x] Ledger capacity is fixed. Conservative coalescing retains exact estimated loss count and never implies continuity or allocates in proportion to dropped frames.
- [x] Base64/header work happens only in the worker after acceptance. Empty, malformed, unsupported, over-limit, missing-IHDR, or no-SOF-within-64-KiB data emits `FrameRejected`; valid JPEG/PNG keeps encoded bytes unchanged and reports header dimensions without `image` or pixel decoding.
- [x] **Invalidated after completion:** this story preserved the acknowledgement token as `source_sequence` and inferred discontinuities. Production Chrome 149 and both canonical final5 traces show constant token `1`; the remediation story removes the field, warning, gap reason, and inference, then adds Krometrail-owned `CaptureOrdinal` ordering.
- [x] Visibility false opens one hidden interval; true or an actual frame closes it; repeated signals coalesce and visible silence is never inferred as a gap.
- [x] `SessionOrigin` happens-before subscriptions/start/first frame. Source timestamps are checked/rounded independently, observed time is captured at return, session time derives only through the fixed origin, observed/session ordering is nondecreasing (`next >= previous`), equal samples are accepted, and wall time is absent.
- [x] The parent `CaptureStreamState` transition table is implemented exhaustively; terminal/invalid transitions cannot restart a stream and observer state events are transition-only.
- [x] Fixed 64-bucket logarithmic ack-latency and inter-frame-cadence histograms remain constant-memory, accept zero-duration samples, and expose deterministic sample-count/nearest-rank p50/p95/p99 bucket bounds plus exact max.
- [x] Defaults (8 active streams × 4 slots × 8 MiB base64 text) and every accepted override stay within the 256 MiB queued-payload ceiling. Checked arithmetic rejects overflow and combinations beyond the ceiling; hard caps are 32/16/16 MiB. Ledgers/histograms are separately fixed-size.
- [x] `CaptureConfig` is the only exported capture type; coordinator/dependencies/target/observer/error/stop/outcomes are crate-private and covered by internal tests.
- [x] Observer/status/log tests reject URL/title/browser key/CDP session/raw params/payload fields at info level.
- [x] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass with no session-wiring story required.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fresh-context deep review independently verified all acceptance criteria, ran 135 workspace tests plus no-default/spike/clippy gates and adversarial histogram/gap probes, and approved the timing-sensitive engine. Lower-risk hardening proposals were parked as `idea-capture-engine-hardening`; no current-cycle blocker remains.

## Execution

- Effective worker: `highest`.
- Review weight: `standard`; this timing-sensitive story intentionally remains at `stage: review` for a fresh host review rather than fast-advancing.
- Capability: `cdpkit-transport` (default), with `--no-default-features` and `cdp-spike-cdpkit` compile-real checks retained.
- File ownership is exclusive to this story in the planned wave. It does not add `BrowserSessionEvent` variants or edit target reducer/model files; the dependent wiring story owns those changes, preserving a compile-real boundary.

## Implementation notes

- Added core acknowledged capture statistics, registry-backed stream states and gap reasons, bounded timing summaries, and validated target status snapshots.
- Added the private capture coordinator and per-target bounded pipeline. The receive path samples observed time, completes `Page.screencastFrameAck`, records acknowledgement timing, and only then parses metadata or attempts `try_send`; the worker owns base64 decoding, bounded JPEG/PNG header inspection, frame construction, and the single `RecordingSink` append path. The implemented `source_sequence` interpretation is superseded and tracked by the remediation story; ack-first ordering remains valid.
- Defaults remain 8 active streams × 4 queued payloads × 8 MiB text, with checked 256 MiB aggregate validation and fixed-size ledgers/histograms. No image dependency, pixel decoder, persistence implementation, session wiring, production start/stop composition, or analysis code was introduced.
- Verification: workspace format/check/test/clippy, `krometrail-cdp` no-default check, and the cdpkit spike-feature check pass. Internal tests cover ack barriers and saturation, parser rejection, transitions, gap coalescing, clock monotonicity, histogram bounds, configuration caps, and privacy-safe status surfaces.
- Simplification: reused `CdpTransport`, `RecordingSink`, `MonotonicClock`, `IdSource`, `SessionOrigin`, and existing frame/gap contracts; kept the coordinator, error, stop/outcome, transport context, and observer types crate-private and avoided a second queue or image abstraction.
