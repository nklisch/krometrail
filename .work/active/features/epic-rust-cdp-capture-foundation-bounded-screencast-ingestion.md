---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
kind: feature
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Bounded Screencast Ingestion

## Implementation discovery (2026-07-13)

The opt-in production test invalidated a load-bearing premise and correctly bounced this feature to drafting instead of weakening the evidence:

- Chrome 149 exposes `Page.screencastFrame.params.sessionId` as the opaque integer that must be echoed to `Page.screencastFrameAck`, not as usable frame-order or continuity evidence. It was constant `1` in the live production diagnostic. The committed canonical final5 traces independently contain 101 sampled real screencast events on Linux and 101 on macOS, with `sessionId: 1` in every event.
- The earlier B1 adjudication was wrong: it dismissed the canonical trace as scripted and promoted protocol wording into behavior that real Chrome does not provide. `source_sequence`, `SourceSequenceDiscontinuity`, and strict Chrome-number assertions therefore fabricate a guarantee and must be removed.
- Production Chrome captured zero frames because initial `ProbeInitialVisibility` accepted only one raw `Runtime.evaluate` result shape. The reconnect path already accepts both cdpkit shapes. Initial reconciliation must do the same and must not enter `Ready` while a recordable target's visibility is still unresolved.
- The already-done engine and wiring stories implemented and approved the invalid premise. Their historical work remains recorded, but their affected acceptance notes are explicitly superseded. A new remediation story owns the production correction; the unfinished real-Chrome story cannot proceed until it lands.

`KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real --locked -- --nocapture` remains red with four bounded zero-capture liveness failures. No assertion is weakened into a pass.

## Brief

Turn each supervised page target into a production live visual stream without allowing storage or later image work to stall CDP. Acknowledge each received screencast frame to completion before decoding or attempting bounded ingestion, then assign honest Krometrail-owned frame ordering and expose per-target statistics for received, acknowledged, accepted, dropped, and persisted frames.

Normalize every observation onto a monotonic session clock while preserving Chrome source time and daemon observed time as distinct evidence. Queue saturation, malformed frames, visibility pauses, disconnection, downstream rejection, acknowledgement failure, and shutdown abandonment produce explicit gaps. Krometrail does not infer a gap from Chrome's acknowledgement token or from silence on an otherwise visible target.

Cancellation stops acceptance, drains or reports accepted work under one bounded flush policy, and leaves durable persistence behind the existing port. This feature does not implement segments, retention, temporal artifacts, browser-control actions, or an MCP surface.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position: consumes supervised flat target sessions and supplies the validated live frame stream to later storage work
- Inherited decisions: exact cdpkit 0.4.0 remains behind `krometrail-cdp::transport`; Krometrail owns acknowledgement ordering, bounded handoff, explicit gaps, reconnect reconstruction, cancellation, and flush

## Foundation and evidence references

- `docs/SPEC.md` — Sessions and Targets, Continuous Visual Capture, Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Time Model, Frame Ingestion, Capture Tasks, Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Source Frames and Capture Gaps
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity
- `docs/evidence/cdp-transport/v2/cdpkit-linux.json` and `cdpkit-macos.json` — canonical final5 real-browser traces; 101 sampled `Page.screencastFrame` events per platform, all with acknowledgement token `1`
- `.agents/skills/rust-cdp-transport/SKILL.md` — exact cdpkit boundary, ack-first contract, named-params limitation, and final5 qualification identity

## Scope

- Keep one recording identity and one fixed monotonic `SessionOrigin` per production browser session, sampled before capture subscriptions, `Page.startScreencast`, or the first frame.
- Start one stream only for an exact supervised target attachment after session `Ready`, target `Attached`, flat transport session known, and initial visibility observed.
- Subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before `Page.startScreencast`; request every available frame with `everyNthFrame: 1`.
- On each returned frame, timestamp receipt, read `params.sessionId` only as `ack_token`, complete `Page.screencastFrameAck`, assign the next Krometrail `CaptureOrdinal`, then parse metadata and attempt non-blocking bounded handoff.
- Keep one bounded queue, bounded gap ledger, worker, fixed timing histograms, and status snapshot per target attachment. Preserve a Krometrail-owned ordinal allocator per `(SessionId, TargetId)` across reconnect attachment generations.
- Preserve globally unique `FrameId`, `SessionId`, `TargetId`, `CaptureOrdinal`, optional Chrome source time, observed time, normalized session time, image and viewport dimensions, device scale, format, and truthful warnings.
- Remove the impossible `CaptureWarning::SourceSequenceDiscontinuity` and `CaptureGapReason::SourceSequenceDiscontinuity`. Add no local-ordinal discontinuity warning. Explicit loss and lifecycle gaps are authoritative.
- Repair initial visibility decoding before `Ready`; both cdpkit raw result shapes use one parser shared with reconnect. Invalid/error results become target-local probe failure and exact-session detach rather than silent `Unknown` visibility.
- Preserve target isolation, generation fencing, visibility intervals, reconnect identity, acknowledgement timing, bounded memory, and the approved one-absolute-deadline shutdown sequence.
- Keep deterministic fake coverage and finish opt-in production Chrome coverage against the retained fixture.

## Non-goals

- No claim that `Page.screencastFrame.params.sessionId` orders frames, changes per frame, detects skipped browser frames, or has meaning after acknowledgement.
- No claim that `CaptureOrdinal` detects Chrome-side or cdpkit-side loss. It orders only successfully acknowledged frame events observed by Krometrail.
- No gap inferred from ordinal arithmetic, a quiet visible page, source timestamps, cadence, or the ack token.
- No SQLite, segments, retention, payload offsets, crash recovery, image transcoding, pixel decode, visual analysis, temporal queries, control actions, MCP tools, or public command/configuration surface.
- No cdpkit fork, chromey/owned fallback, spike-runtime reuse, fake-success sink, or weakening of final5 thresholds.

## Execution policy and grounding

- **Driver:** direct redesign requested after implementation discovery; no questions or subagents were used.
- **Effective review weight:** standard. The caller prohibited subdelegation, so no design-time independent advisory ran; this does not block design completion. Feature/epic review remains required later.
- **Dispatch rationale:** direct reads covered the feature and children, parent epic, five foundation docs, canonical final5 Linux/macOS reports, current core frame/gap contracts, capture pipeline, session wiring, real-Chrome test, and rust-CDP skill.
- **Rolling Foundation:** design-first correction. Standing claims that Chrome supplies a sequence identifier or observable sequence discontinuities are false and are corrected with this redesign. Git retains the superseded claim.

## Design decisions

- **Corrected B1 adjudication:** accept B1. Protocol prose calling the integer a frame number does not outweigh canonical Linux/macOS Chrome 149 observations. The field is an opaque acknowledgement token in Krometrail and is never persisted, logged, compared, or exposed as metadata. Parse it as the protocol/generated API's signed `i64` integer and echo the exact value; do not add positivity or monotonicity validation.
- **Krometrail ordering name and semantics:** introduce `CaptureOrdinal`, a non-zero Krometrail-owned ordinal. It increases by one for every successfully acknowledged frame event observed for one `(SessionId, TargetId)`, including events later rejected or dropped. It continues across attachment generations and resets only with a new target identity or recording session. Missing ordinals among retained frames can correlate with explicit Krometrail loss records, but ordinal arithmetic never creates a gap and says nothing about frames Chrome never emitted or cdpkit never delivered.
- **Attachment generation in frame metadata:** do not add it. The current core frame metadata contract is `CapturedFrame` (returned by `EncodedFrame::metadata()`); there is no separate `FrameMetadata` type. `SessionId + TargetId + CaptureOrdinal` is unambiguous across reconnect because the ordinal does not reset. Attachment generation remains adapter lifecycle/status data, while `BrowserDisconnected` gaps and session time preserve the evidence boundary. Adding generation would leak transport reconstruction into the adapter-neutral recording model without improving identity or ordering.
- **Ordinal allocation point:** allocate only after successful acknowledgement and before post-ack parse/handoff. Ack failure receives no ordinal and emits `AcknowledgementFailed`; every assigned ordinal therefore identifies one acknowledged Krometrail observation. The coordinator owns one checked allocator per target identity so a replacement runtime cannot reset it.
- **Gap vocabulary:** delete `SourceSequenceDiscontinuity` from warning and gap registries rather than redefining it. Add `AcknowledgementFailed` as the precise replacement for a returned frame that cannot complete acknowledgement. Existing `IngestionQueueSaturated`, `FrameRejected`, `PersistenceRejected`, `TargetHidden`, `ScreencastPaused`, `BrowserDisconnected`, and `CaptureStopped` retain their concrete meanings. Counts are exact only for known local frame loss; lifecycle intervals may have no missing-frame estimate.
- **Initial visibility:** one `parse_visibility_result(&Value) -> Result<TargetVisibility, VisibilityProbeError>` accepts both `/result/result/value` and `/result/value`, and only `visible`/`hidden` values. Initial and reconnect paths share it. Every initial probe feeds either `VisibilityChanged` or a new `InitialVisibilityProbeFailed` reducer input; the latter preserves the exact flat session long enough to detach and marks only that target failed. `InitialReconciliationCompleted` rejects any nonterminal recordable target whose visibility is unresolved, so `Ready` cannot silently strand `Unknown` visibility.
- **Acceptance point:** accepted means an acknowledged raw frame was placed in that target generation's bounded queue. Acknowledgement is not acceptance or retention. Counters retain `received >= acknowledged >= accepted + dropped` and `persisted <= accepted`; acknowledgement failure is a gap and stream failure, not a dropped acknowledged frame.
- **Ack-first boundary:** receipt timestamp, token validation, and ack completion remain ahead of ordinal allocation, source metadata parsing, payload-size checks, queue occupancy, gap-ledger work, base64/header parsing, and sink work. Ack latency remains return-to-completion only.
- **Boundedness:** retain the validated per-target Tokio channel, fixed gap ledger, fixed histograms, and checked 256 MiB aggregate queued-base64 ceiling. Never await frame-channel capacity. cdpkit's private unbounded subscriber remains an explicit unmeasurable upstream limitation.
- **Clocks:** preserve Chrome `SourceTime`, daemon `ObservedTime`, and normalized `SessionTime` independently. Session origin precedes capture setup; observed/session values are nondecreasing and can be equal. Source time cannot order unrelated daemon observations.
- **Lifecycle and shutdown:** retain reducer-owned Start/Stop/Suspend/Resume capture effects, exact generation fencing, target-local failure, visibility intervals, reconnect restoration, stop-before-acceptance, one aggregate deadline, one session flush, detach, `Browser.close`, and managed-process cleanup. No remediation may reopen these approved semantics unnecessarily.

## Architectural choice

### Option 1: Keep `source_sequence` but document it as unreliable

This minimizes code churn but leaves a public field whose name invites exactly the false continuity inference that caused the failure. Rejected.

### Option 2: Remove sequence-like metadata and rely only on time plus `FrameId`

`SessionTime` already provides authoritative timeline ordering and `FrameId` identity. This is honest and smallest, but downstream deterministic ordering among equal clock readings would need a tie-breaker outside the frame contract. It also makes known acknowledged-but-dropped positions harder to correlate. Viable, but not chosen.

### Option 3: Replace it with Krometrail-owned `CaptureOrdinal`

Assign a per-target-session ordinal after acknowledgement, continue it across reconnect generations, and keep all loss claims in explicit gaps. Chosen because it gives deterministic tie-breaking and local accounting without pretending Chrome continuity. The narrower alternative was considered; the ordinal earns its place because equal monotonic samples are valid and later storage requires stable per-target order.

## Revised implementation units

### Unit 1: Correct core metadata and engine semantics

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-contract-remediation`

**Files:**
- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptureOrdinal(NonZeroU64);

impl CaptureOrdinal {
    pub fn new(value: u64) -> Result<Self>;
    pub const fn get(self) -> u64;
}

pub enum CaptureWarning {
    MissingSourceTime,
    SourceTimestampRounded,
    ViewportMetadataIncomplete,
}

define_stable_enum! {
    pub enum CaptureGapReason {
        IngestionQueueSaturated => "ingestion_queue_saturated",
        PersistenceRejected => "persistence_rejected",
        AcknowledgementFailed => "acknowledgement_failed",
        TargetHidden => "target_hidden",
        ScreencastPaused => "screencast_paused",
        BrowserDisconnected => "browser_disconnected",
        CaptureStopped => "capture_stopped",
        FrameRejected => "frame_rejected",
    }
}

impl CapturedFrame {
    pub fn capture_ordinal(&self) -> CaptureOrdinal;
    // source_sequence() is removed; constructor and serde wire use capture_ordinal.
}
```

The coordinator obtains one shared checked ordinal allocator for each `TargetId` in the recording session. `frame_reader` parses the CDP integer as `ack_token: i64`, uses it only in the ack command, then requests the next ordinal. Overflow is an explicit stream failure; no saturating/repeated ordinal is permitted. `RawFrame` and `CapturedFrame` carry `capture_ordinal`; the ack token does not cross the reader boundary.

**Acceptance criteria:**

- [x] No production/core/test symbol or serialized field named `source_sequence` or `SourceSequenceDiscontinuity` remains in this feature's surfaces.
- [x] The ack token appears only in validation and `Page.screencastFrameAck` parameters; it is not persisted, logged, compared, exposed, or copied into `RawFrame`.
- [x] `CaptureOrdinal` is non-zero, serde-validating, strictly allocated per `(SessionId, TargetId)` after ack, continuous across attachment generations, and deterministic when clock values are equal.
- [x] Ack failure emits `AcknowledgementFailed`, hands off nothing, assigns no ordinal, and fails only that stream.
- [x] Saturation/rejection/persistence/stop gaps remain explicit and bounded; no ordinal gap is inferred and no missing-browser-frame estimate is fabricated.
- [x] Existing ack-first barriers, queue/ledger/histogram bounds, image-header handling, three clocks, visibility handling, status invariants, and no-default build remain green.

### Unit 2: Repair visibility reconciliation before Ready

**Story:** same remediation story; this is one cohesive correction because real capture cannot verify the metadata change until initial lifecycle reaches capture start.

**Files:**
- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/tests/session_capture.rs`

```rust
fn parse_visibility_result(value: &serde_json::Value)
    -> Result<TargetVisibility, VisibilityProbeError>;

pub enum SupervisorInput {
    // existing inputs
    InitialVisibilityProbeFailed { target_key: String },
}
```

Use the parser in initial and reconnect flows. Initial `ProbeInitialVisibility` always reduces success or failure before the effect queue completes. Probe failure detaches the exact flat session and marks the target failed. Guard `InitialReconciliationCompleted` against any nonterminal recordable target that is still pending/Unknown.

**Acceptance criteria:**

- [x] Both cdpkit raw result shapes (`/result/result/value` and `/result/value`) produce observed `Visible`/`Hidden` state before initial `Ready`.
- [x] Command failure, malformed shape, or unsupported value cannot silently leave an attached target `Unknown`; it produces target-local failure and exact-session detach.
- [x] `InitialReconciliationCompleted` cannot transition to `Ready` while a nonterminal recordable target has unresolved initial visibility.
- [x] Reconnect uses the same parser and preserves its existing transactional, bounded behavior.
- [x] Existing reducer-owned capture effects, target isolation, generation fencing, visibility gaps, and aggregate shutdown/lifecycle tests remain unchanged and green.

### Unit 3: Real-Chrome fidelity and shutdown evidence

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity`

**Depends on:** remediation story

**File:** `crates/krometrail-cdp/tests/capture_real.rs`

Replace strict Chrome-token assertions with `CaptureOrdinal` assertions. Keep the canonical token value as corroborating diagnostic evidence only if read locally; do not make it part of persisted production metadata. Verify ordinals increase within a target and continue across reconnect while attachment generation increases separately.

**Acceptance criteria:**

- [x] Managed Chrome reaches Ready only after initial visibility resolves and yields at least 30 non-empty JPEG frames under bounded timeout.
- [x] Frames have unique `FrameId`, expected session/target identity, strict per-target `CaptureOrdinal`, nondecreasing observed/session times, optional source time, coherent dimensions/scale, and unchanged compressed bytes.
- [x] Two targets have independent ordinals/status/gaps; a reconnect preserves `TargetId`, advances attachment generation, continues that target's ordinal, and records `BrowserDisconnected` without a Chrome-sequence claim.
- [x] Saturation proves ack-first bounded loss accounting; visibility and shutdown/lifecycle assertions remain as previously designed.
- [x] The opt-in Chrome command and all workspace/default/no-default/spike gates pass without altering final5 evidence or thresholds.

## Child-story correction policy

The engine and supervised-wiring stories remain `done` as an audit record of implementation and review that actually occurred. They are not treated as valid completion evidence for the revised feature:

- engine acceptance around Chrome frame numbers, source-sequence gaps, and stored metadata is superseded by the remediation story;
- wiring acceptance around initial visibility is superseded where it failed to parse both production result shapes;
- all unaffected ack-first, boundedness, clock, lifecycle, reconnect, privacy, and shutdown acceptance remains reusable evidence.

The feature cannot advance to review until remediation and real fidelity are both done.

## Implementation order and dependencies

1. Historical engine — done, but affected semantics explicitly superseded.
2. Historical supervised wiring — done, but initial visibility handling explicitly superseded.
3. `...-contract-remediation` — depends on supervised wiring; corrects core/engine and initial visibility in one compile-real stride.
4. `...-real-chrome-fidelity` — depends on remediation; re-runs honest production evidence.

A single remediation story is intentionally minimal. Splitting metadata and visibility would create a false intermediate state: the production gate cannot exercise corrected ordering until visibility permits capture, and both corrections touch the same capture/session integration boundary.

## Simplification and elimination

- Remove `previous_sequence`, `sequence_gap`, `source_sequence`, `SourceSequenceDiscontinuity`, and every associated warning/gap/test branch.
- Reuse one visibility-result parser for initial attach and reconnect instead of retaining divergent JSON-pointer logic.
- Do not add attachment generation to adapter-neutral frame metadata, a second frame metadata type, a browser-source counter, or a compatibility alias for `source_sequence`.
- Retain the existing transport, sink, clock, ID, coordinator, target reducer, status, and shutdown abstractions; this remediation does not earn a new service or queue.
- Keep final5 evidence byte-for-byte unchanged. Correct interpretation in standing docs and the rust-CDP reference rather than rewriting evidence.

## Testing

### Regression tests

- Canonical-style constant ack tokens across many fake events produce increasing Krometrail ordinals, no discontinuity warning/gap, and ack commands that echo the token exactly.
- A reconnect with a new attachment generation continues the same target ordinal after an explicit `BrowserDisconnected` gap.
- Both raw `Runtime.evaluate` result shapes resolve before Ready; malformed/error responses fail and detach the exact target instead of silently becoming Ready.

### Existing contract tests retained

- Ack-completion barrier before parse/handoff, blocked-sink saturation, bounded ledgers/histograms/memory, target isolation, generation fencing, visibility intervals, source/observed/session clocks, and one aggregate shutdown deadline.
- Core serde/invariant coverage for frame metadata, gap registry, status, and object-safe browser port.
- Opt-in managed/attached Chrome, two-target, saturation, reconnect, visibility, and cleanup scenarios.

### Tests deliberately not added

- No test that Chrome's token is always `1`; final5 and Chrome 149 establish current behavior, but Krometrail intentionally assigns no semantics beyond echo-for-ack.
- No inferred upstream-loss test, cadence-gap detector, or ordinal-discontinuity warning.
- No image-pixel, storage durability, retention, artifact, browser-control, or host-speed percentile tests.

## Feature acceptance

- [x] `Page.screencastFrame.params.sessionId` is ack-only everywhere; canonical final5 and production Chrome are described honestly, and corrected B1 adjudication is recorded.
- [x] Core frames expose Krometrail `CaptureOrdinal` rather than a fabricated Chrome source sequence; attachment generation stays in adapter lifecycle/status, not frame metadata.
- [x] Impossible source-sequence warning/gap variants and logic are removed; all known local loss and lifecycle interruptions remain explicit without inferring unknown Chrome loss.
- [x] Initial visibility handles both cdpkit raw result shapes and cannot enter Ready unresolved.
- [x] Ack-first ordering, bounded queue/ledger/histograms/memory, three clocks, target isolation, reconnect fencing, visibility, privacy, and one-deadline shutdown remain intact.
- [x] Done child stories clearly identify superseded acceptance; remediation is tracked as implementing and real fidelity depends on it.
- [x] Real Chrome yields truthful frame/loss/lifecycle evidence, and all default/no-default/spike/workspace gates pass in the owning implementation stories.

## Risks and pre-mortem

- **Ordinal could be mistaken for browser completeness.** Mitigation: the type and docs define it as Krometrail observation order only; no gap derives from it; explicit local/lifecycle gaps remain authoritative.
- **Reconnect could reset or race the ordinal.** Mitigation: allocator ownership is `(SessionId, TargetId)` at coordinator scope, old readers are fenced before replacement, and real/fake reconnect tests require continuation across a higher attachment generation.
- **Removing a stable enum/field changes serialized contracts.** This repository has not shipped the capture surface. Keeping a compatibility alias would preserve a lie and violate code economy. Update all internal fixtures atomically and provide no shim.
- **Visibility shape may drift again.** Mitigation: one narrow parser supports the two observed cdpkit shapes and rejects unknown shapes explicitly. Do not recursively search arbitrary JSON or default to visible.
- **Gap persistence can fail.** Existing status/events report the gap before sink persistence and shutdown remains bounded; durable recovery belongs to storage.
- **cdpkit's subscriber remains unbounded.** The ack reader stays minimal and final5 saturation remains the qualification evidence. Demonstrated accumulation reopens the selected transport under existing fallback rules.
- **Least certain:** Chrome visibility behavior across headed/headless macOS high-DPI. The dependent cross-platform smoke remains the authority; this feature only requires truthful observed visibility and no inferred silence gap.

## Implementation summary (2026-07-13)

- Landed a bounded per-target capture engine with acknowledgement completion before metadata parsing or non-blocking handoff, fixed-capacity queues/ledgers/histograms, worker-only decode/header parsing, three-clock normalization, explicit known-loss gaps, and aggregate memory validation.
- Integrated capture through the target reducer/effect executor as the lifecycle source of truth, with exact attachment contexts, initial visibility gating, target-local failure isolation, reconnect fencing, and one absolute stop deadline through capture drain/flush, detach, browser close, and process cleanup.
- Corrected a live-evidence design error: Chrome's signed screencast `sessionId` is an opaque acknowledgement token, not a frame sequence. Core evidence now uses validating per-session/target `CaptureOrdinal` continuous across attachment generations; impossible source-sequence continuity claims and gaps were removed.
- Repaired both cdpkit visibility result shapes and unresolved-visibility Ready guarding.
- Proved production behavior through repeated real Chrome 149 runs: 30-frame managed fidelity, three-target isolation, exact bounded saturation accounting, generation 1→2 reconnect with continuous ordinals, ownership-correct managed/attached stop, and no process/profile references.
- Workspace/default/no-default/spike/clippy/doc gates pass. Four child stories are `done`; lower-risk capture-engine and test-root hygiene proposals are parked.

## Feature review findings (2026-07-13)

GLM completeness review approved. A second cross-model seam review found two receiver-confirmed material gaps: acknowledgement latency starts after token extraction rather than at frame return, and stopped target runtimes remain in the coordinator map, violating the stated O(active streams) memory/status bound under target churn. The parent returned to implementing for `...-feature-review-remediation`; that story now measures receipt-to-ack completion exactly, evicts terminal runtimes safely, exposes only highest-generation sorted statuses, preserves/reclaims ordinal state by lifecycle, and proves final status publication before removal. Exact opt-in Chrome passed twice after repair.

## Final feature review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** lower-risk engine and test-harness hardening is parked in the backlog.

**Notes:** Independent GLM completeness and Kimi adversarial reviews covered the feature as a whole. The adversarial findings were repaired and re-reviewed: receipt-to-ack timing now matches the documented metric, target churn cannot retain terminal runtime/status state, final stopped status remains observable before removal, ordinal continuity survives nonterminal replacement, and terminal/session cleanup is bounded. Current gates pass with 163 workspace tests and repeated 5/5 real-Chrome runs; no material finding remains.
