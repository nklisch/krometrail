---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-contract-remediation
kind: story
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Correct screencast ordering and initial visibility

## Why this remediation exists

Production Chrome 149 and both committed canonical final5 traces invalidate the completed engine's source-sequence premise. `Page.screencastFrame.params.sessionId` is the opaque integer echoed to `Page.screencastFrameAck`; all 101 sampled Linux events and all 101 sampled macOS events use constant value `1`. It is not frame continuity evidence.

The same production run exposed a wiring defect: initial `ProbeInitialVisibility` accepts only `/result/result/value`, while this cdpkit path returned the already-supported reconnect shape `/result/value`. The session reached reconciliation without observed visibility, so capture never started and all four real-Chrome scenarios timed out with zero frames.

This story corrects the already-done engine and wiring in one compile-real stride. It does not reopen their unaffected ack-first, boundedness, clock, target-lifecycle, reconnect, privacy, or shutdown behavior.

## Scope

### Core and capture engine

- Replace `CapturedFrame::source_sequence: u64` with a validating non-zero `CaptureOrdinal` and rename constructor, getter, serde wire, internal `RawFrame`, fixtures, and tests accordingly.
- Assign ordinals after successful acknowledgement and before parse/handoff. The allocator is owned per `(SessionId, TargetId)` by the coordinator and continues across attachment generations.
- Parse the CDP integer as `ack_token: i64`, matching the protocol/generated API, and keep it only long enough to send `Page.screencastFrameAck`. Echo the exact value without positivity/monotonicity assumptions. Never persist, log, compare, expose, or copy it into worker input.
- Delete `CaptureWarning::SourceSequenceDiscontinuity`, `CaptureGapReason::SourceSequenceDiscontinuity`, `previous_sequence`, `sequence_gap`, and all inferred missing-frame logic/tests.
- Add `CaptureGapReason::AcknowledgementFailed`. Ack failure assigns no ordinal, hands off nothing, emits the precise gap/status evidence available, and fails only that target stream.
- Keep explicit local/lifecycle gaps for saturation, rejection, downstream failure, hidden targets, screencast pause, browser disconnect, and stop abandonment. Do not derive gaps from ordinal arithmetic, cadence, source timestamps, token values, or visible silence.

### Initial visibility and Ready

- Extract one strict `parse_visibility_result` helper used by initial attach and reconnect. Accept `/result/result/value` and `/result/value`; map only `visible` and `hidden`.
- Add `SupervisorInput::InitialVisibilityProbeFailed { target_key }`. On command error, malformed shape, or unsupported value, preserve the exact attached flat session long enough to detach it, mark that target failed, and keep unrelated targets progressing.
- Make every `ProbeInitialVisibility` effect reduce either `VisibilityChanged` or `InitialVisibilityProbeFailed` before the effect queue completes.
- Guard `InitialReconciliationCompleted`: a nonterminal recordable target with unresolved/pending `Unknown` visibility prevents the Connecting → Ready transition.
- Preserve reducer ownership of lifecycle/capture binding and the approved Start/Stop/Suspend/Resume effects. Do not add polling or a second reconciliation loop.

## Required files

- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`
- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/tests/session_capture.rs`

Do not edit the real-Chrome test in this story; its dependent story owns the final production evidence. Do not edit canonical final5 JSON, spike code, fixture content, storage, temporal vision, MCP, or root command/configuration surfaces.

## Contract

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptureOrdinal(NonZeroU64);

impl CaptureOrdinal {
    pub fn new(value: u64) -> Result<Self>;
    pub const fn get(self) -> u64;
}

impl CapturedFrame {
    pub const fn capture_ordinal(&self) -> CaptureOrdinal;
}

fn parse_visibility_result(
    value: &serde_json::Value,
) -> Result<TargetVisibility, VisibilityProbeError>;
```

`CapturedFrame` remains the adapter-neutral core frame metadata contract exposed by `EncodedFrame::metadata()`; do not introduce a second `FrameMetadata` wrapper. Attachment generation remains in adapter status/lifecycle and is not added to frame metadata because `CaptureOrdinal` is continuous across generations.

## Acceptance criteria

- [x] No production/core/test symbol or serialized field named `source_sequence` or `SourceSequenceDiscontinuity` remains in the bounded-ingestion surface.
- [x] The CDP `sessionId` value is used only as `ack_token` for frame acknowledgement and is absent from `RawFrame`, `CapturedFrame`, status, gaps, logs, and persistence calls.
- [x] `CaptureOrdinal` validates non-zero construction/deserialization, is assigned strictly after ack completion, increases by one per acknowledged observation for one `(SessionId, TargetId)`, and continues across higher attachment generations.
- [x] Constant-token fake events produce strict ordinals without warning/gap; equal clock readings remain valid and ordinal provides deterministic per-target tie-breaking.
- [x] Ack failure assigns no ordinal, hands off nothing, emits `AcknowledgementFailed`, and leaves unrelated streams live.
- [x] Queue saturation, malformed/oversized frame rejection, persistence rejection, visibility, browser disconnect, and bounded shutdown abandonment retain explicit truthful gaps and bounded accounting. No inferred upstream-loss path exists.
- [x] Both observed cdpkit `Runtime.evaluate` result shapes resolve `Visible`/`Hidden` before Ready through one shared parser.
- [x] Initial visibility command/shape/value failure detaches the exact flat session and marks only that target failed; it cannot silently leave an attached `Unknown` target.
- [x] `InitialReconciliationCompleted` rejects unresolved initial visibility on nonterminal recordable targets; capture still starts exactly once only for Ready/Attached/Visible exact generations.
- [x] Existing acknowledgement barriers, queue/ledger/histogram bounds, three clocks, image-header behavior, target isolation, generation fencing, reconnect restoration, privacy, and one-absolute-deadline shutdown tests remain green.
- [x] Core serde/registry tests, capture fake tests, supervised-session integration tests, workspace fmt/check/test/clippy, no-default check, and cdpkit spike regression pass.

## Implementation notes

- Replaced the public frame field and serde wire with validating transparent `CaptureOrdinal`; the coordinator owns a checked `(SessionId, TargetId)` registry that fences old attachment generations before allocating the next ordinal. Equal clock samples remain valid and ordinals provide the local deterministic tie-breaker.
- Kept the CDP `sessionId` as a signed `i64` acknowledgement token only. The receive path echoes it exactly, emits an exact one-frame `AcknowledgementFailed` gap on invalid/failed/timed-out acknowledgement, and never copies it into raw or persisted frame metadata.
- Removed source-sequence discontinuity state, warning, gap, inference, and tests. Constant-token scripted events now produce strict local ordinals, including continuation through a fenced higher attachment generation.
- Unified initial and reconnect visibility decoding through the strict two-envelope parser. Initial probe failures reduce to target-local failure plus exact-session detach, and Ready rejects unresolved nonterminal visibility.
- Post-completion traceability: the engine and supervised-wiring stories remain historically `done` with their affected claims marked superseded; this remediation is the authoritative correction for their frame-order and initial-visibility contracts.
- Verification: `cargo fmt --all -- --check`; full workspace check/test; no-default check; cdpkit spike check; workspace clippy with `-D warnings`; documentation build. The opt-in real-Chrome fidelity run remains owned by the dependent story.
- Discrepancy from required files: the already-compiled dependent `capture_real.rs` needed a two-line getter rename from `source_sequence()` to `capture_ordinal()` so workspace gates remained compile-real. Its live assertions, lifecycle behavior, and final evidence remain owned by the dependent story.

## Dependencies and handoff

Depends on the completed supervised-wiring story, which itself depends on the completed engine. The real-Chrome fidelity story depends on this remediation and must replace every strict Chrome-token/source-sequence assertion with `CaptureOrdinal` assertions before it can return to review.

## Execution

- Effective worker: highest.
- Review weight: standard at the parent feature; this correction changes a core serialized contract and production readiness, so it should receive fresh-context feature review through the normal lane.
- One story is intentional: the corrected production metadata cannot be validated until initial visibility allows capture, and splitting these tightly coupled regressions would leave a misleading green intermediate state.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** ordinal exhaustion has no explicit gap at the practically unreachable `u64::MAX` boundary; an acknowledged stale-generation callback truthfully increments only the dying runtime's transient stats; reconnect visibility parsing is intentionally stricter than the superseded fallback-to-visible behavior.

**Notes:** Fresh-context three-round review verified every acceptance criterion, signed opaque acknowledgement handling, coordinator-scoped ordinal fencing across generations and races, visibility failure isolation, Ready guarding, and all unaffected bounded/timing/shutdown/privacy contracts. Workspace gates passed with 156 tests. No material finding remains.
