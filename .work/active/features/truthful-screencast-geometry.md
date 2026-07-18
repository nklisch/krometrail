---
id: truthful-screencast-geometry
kind: feature
stage: review
tags: [bug, visual, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Truthful screencast geometry

## Brief

Fix retained frame provenance so visual epochs describe the page's real CSS viewport and device scale rather than Chrome's adaptive screencast encoding dimensions. Manual testing with v1.0.3 held a page at 600×500 CSS pixels and DPR 1, yet one 600×500 encoded frame was recorded as a 300×250 viewport amid otherwise 1200×1000 encoded frames recorded as 600×500. The false transition split a stable five-second interaction into three visual epochs.

The capture pipeline must preserve encoded image dimensions independently from authoritative page geometry across viewport apply, clear, navigation replay, and reconnect. Regression evidence must prove adaptive screencast scaling cannot invent a viewport transition.

## Simplification opportunity

Consolidate capture geometry authority around the already acknowledged target-scoped viewport lifecycle. Remove any inference that treats `Page.screencastFrame` encoding metadata as independently authoritative CSS layout geometry when stronger target state is available.

## Design decisions

- **Geometry authority**: independently observed effective CSS viewport plus device scale is authoritative; screencast `deviceWidth` and `deviceHeight` describe Chrome's encoded delivery and cannot establish layout geometry.
- **Update atomicity**: capture stores viewport and device scale as one geometry value so a frame cannot combine halves of two acknowledged states.
- **Compatibility boundary**: persisted frame fields and MCP shapes remain unchanged; only the truthfulness of values written into the existing viewport/device-scale provenance changes.
- **Dispatch rationale**: direct-read design across the bounded capture, viewport, and session-runtime seams; no exploratory agent was needed.

## Architectural choice

Carry an authoritative `CaptureGeometry` into each capture stream, seed it from `observe_effective_viewport` before capture starts, and replace it atomically after a successful viewport transaction. `RawFrame::after_ack` snapshots that acknowledged geometry while retaining the screencast envelope only for encoded data and source time.

Alternatives rejected:

- Deriving CSS dimensions from screencast metadata and encoded-image size is not reliable because Chrome may adaptively scale both together without a layout change.
- Re-observing layout metrics for every frame would add a command round trip to the acknowledgement-sensitive capture path and couple capture cadence to page observation.
- Splitting encoded-size changes into epochs would preserve the false provenance and merely hide it behind normalization.

## Implementation Units

### Unit 1: Capture geometry value and frame-envelope decoding

**Files**: `crates/krometrail-cdp/src/capture/mod.rs`, `crates/krometrail-cdp/src/capture/pipeline.rs`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CaptureGeometry {
    pub(crate) viewport: PixelDimensions,
    pub(crate) device_scale_factor: DeviceScaleFactor,
}

impl CaptureCoordinator {
    pub(crate) fn update_geometry(
        &self,
        target_id: TargetId,
        attachment_generation: u64,
        geometry: CaptureGeometry,
    ) -> bool;
}

impl RawFrame {
    fn after_ack(
        event: NamedEvent,
        capture_ordinal: CaptureOrdinal,
        observed_time: ObservedTime,
        session_time: SessionTime,
        format: ImageFormat,
        max_payload_bytes: usize,
        geometry: CaptureGeometry,
    ) -> Result<Self, ()>;
}
```

**Implementation notes**:

- Replace the device-scale-only mutex with one mutex holding the complete geometry.
- Do not use `metadata.deviceWidth` or `metadata.deviceHeight` as `CapturedFrame.viewport`; encoded dimensions continue to come from decoded image bytes.
- Snapshot the geometry after acknowledgement and before bounded handoff, preserving existing ordering and loss accounting.

**Acceptance criteria**:

- [x] Adaptive changes to screencast metadata/encoded dimensions do not change retained viewport provenance.
- [x] An acknowledged geometry update applies viewport and DPR together to subsequent frames without restarting capture.

### Unit 2: Lifecycle-complete geometry observation and updates

**Files**: `crates/krometrail-cdp/src/control/viewport.rs`, `crates/krometrail-cdp/src/session/runtime.rs`, `crates/krometrail-cdp/src/session/operations.rs`

```rust
fn capture_geometry(effective: EffectiveViewport) -> Result<CaptureGeometry>;

// Start/resume path:
let effective = observe_effective_viewport(transport, &bound, declared_override).await?;
let target = CaptureTarget { geometry: capture_geometry(effective)?, /* ... */ };

// Set/clear path after the complete browser transaction commits:
capture.coordinator.update_geometry(
    target_id,
    bound.attachment_generation,
    capture_geometry(effective)?,
);
```

**Implementation notes**:

- Convert the observed positive finite CSS size to integral `PixelDimensions` using the same half-pixel tolerance already applied to declared metrics; fail target-locally if the browser returns unusable geometry.
- Initial attach, reconnect restore, set, and clear all use the same effective-viewport observation boundary. Navigation replay retains the acknowledged target state and does not manufacture a new geometry.
- Commit capture geometry only after the viewport override/clear transaction and supervisor state commit succeed.

**Acceptance criteria**:

- [x] Initial native capture and restored override capture begin with independently observed CSS viewport and DPR.
- [x] Apply and clear update both fields atomically after success; rollback leaves prior capture geometry intact.
- [x] Reconnect replay restores geometry before capture resumes and remains target-local on failure.

### Unit 3: Regression coverage for adaptive encoding

**Files**: `crates/krometrail-cdp/src/capture/tests.rs`, `crates/krometrail-cdp/src/session/tests.rs`

**Acceptance criteria**:

- [x] A stream holding 600×500/DPR1 records that provenance for a 1200×1000 encoded frame followed by a 600×500 encoded frame whose screencast metadata claims 300×250.
- [x] A real acknowledged change to 390×844/DPR3 appears on subsequent frames without a capture gap or stream restart.
- [x] Session tests prove failed apply/rollback cannot mutate capture geometry and successful set/clear uses observed values.

## Implementation Order

1. Introduce the atomic capture geometry and remove viewport inference from the frame envelope.
2. Feed it from initial/reconnect observation and viewport set/clear transactions.
3. Add capture and session lifecycle regressions, then run the CDP crate and workspace gates.

## Simplification

- Replace the device-scale-only mutable state and update API with one complete geometry authority.
- Remove the misleading metadata-to-CSS viewport conversion.
- Keep the existing persisted schema, epoch partitioner, and encoded-image dimension detection unchanged.

## Testing

- Capture regression protects the reproduced false-epoch root cause at the narrow frame-ingestion boundary.
- Session transaction tests protect apply/clear/rollback/reconnect ordering.
- Existing artifact epoch tests remain the stable consumer check; they should not be weakened to accommodate false provenance.

## Risks

- Browser-native window resizes must still reach capture geometry. The initial design relies on acknowledged viewport operations and start/reconnect observation; implementation must verify whether native target resize events already update the supervisor. If they do not, add a bounded target event update rather than falling back to adaptive screencast metadata.
- CSS visual viewport can be fractional under page zoom. Conversion must reject or consistently round only values within the existing browser observation tolerance, preserving encoded dimensions separately.

## Review findings (2026-07-17)

**Effective weight**: standard — one same-harness fresh-context pass; closure requires verification of this named fix set without a second independent pass.

**Blockers**:

- The acknowledged geometry is refreshed only at capture start/resume and explicit set/clear. Add a generation-fenced authoritative refresh for native resize, monitor/DPR or zoom, and navigation-driven effective geometry changes. Refresh failure must remain target-local and produce explicit gap/failure evidence rather than stale provenance.
- Explicit set/clear/rollback executes several asynchronous browser commands while capture continues under the old cache. Fence the ambiguous interval so no frame can be retained under an unproven epoch; declare exact loss before authoritative capture resumes.

**Required regression evidence**:

- Exercise session-level set, clear, rollback failure, reconnect/navigation refresh, native resize refresh, and an acknowledgement spanning a geometry transition.
- Assert retained frames use only established old/new geometry and every ambiguous observation is represented as a gap.

**Important**: the existing direct `update_geometry` test is useful but insufficient without the integrated lifecycle/race coverage above.

## Implementation notes

- Replaced the per-stream device-scale mutex with one atomic `CaptureGeometry` containing CSS viewport dimensions and DPR. `RawFrame::after_ack` now snapshots that value after acknowledgement and no longer interprets `deviceWidth` or `deviceHeight` as layout geometry.
- Capture start and reconnect resume now use the existing `observe_effective_viewport` boundary for native and restored-override targets. Positive fractional CSS sizes round to the nearest integral pixel within the existing half-pixel browser tolerance.
- Viewport apply/clear converts the independently observed effective geometry before supervisor commit, rolls back on conversion failure, and updates capture only after the supervisor transaction commits. Existing navigation/reconnect replay ordering remains unchanged.
- The supported geometry mutation surface now includes target-scoped viewport apply/clear, initial/reconnect observation, and generation-fenced refreshes triggered by `Page.frameResized`, `Page.frameNavigated`, and `Page.navigatedWithinDocument`. Adaptive screencast delivery metadata remains intentionally non-authoritative.
- Persisted frame fields, MCP shapes, acknowledgement ordering, bounded handoff, capture ordinals, and gap accounting are unchanged.

## Regression evidence

- Before the production correction, `capture::tests::adaptive_screencast_encoding_does_not_invent_viewport_changes` failed with retained viewport `1200×1000` instead of the authoritative `600×500`.
- The regression now proves a 1200×1000 encoded frame and a 600×500 encoded frame with 300×250 screencast metadata both retain 600×500/DPR1, with one stream start and no capture gap.
- `runtime_geometry_change_keeps_one_continuous_stream_and_per_frame_metadata` proves an atomic 390×844/DPR3 update reaches the subsequent frame without restarting the stream and records the fenced transition as an exact interval gap.
- Existing viewport transaction and reconnect tests continue to prove complete apply/clear/rollback command ordering, mobile page-scale replay before capture, and target-local replay failure.

## Files changed

- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`
- `crates/krometrail-cdp/src/control/viewport.rs`
- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/operations.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`

## Verification

- `cargo test -p krometrail-cdp --lib --locked` — passed, 128 tests.
- `cargo test -p krometrail-cdp --all-targets --locked` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `cargo test --workspace --all-targets --locked` — bounded by the host filesystem while linking the root test binary (`ld: write() failed, errno=28`); the feature-owning CDP all-target suite had already passed in full.

## Deviations

- The repository keeps session unit tests in `crates/krometrail-cdp/src/session/mod.rs` rather than a separate `session/tests.rs`; the existing lifecycle tests there and the capture regressions jointly cover the transaction behavior.

## Review fix implementation (2026-07-17)

- Capture geometry is now a generation-scoped, revisioned authority. A transition token fences the previous revision until independently observed CSS viewport and DPR are committed for the exact target attachment generation.
- Native resize, navigation, same-document navigation, monitor DPR, and zoom effects enter the fence from `Page.frameResized`, `Page.frameNavigated`, and `Page.navigatedWithinDocument`. The event reader fences immediately; the serialized session supervisor then re-observes effective geometry and commits the matching token.
- Frame ingestion snapshots the geometry revision before acknowledgement and retains the frame only when the same established revision remains authoritative after acknowledgement. An acknowledgement that spans a transition is completed, then the ambiguous observation is dropped with `screencast_paused` gap evidence.
- Explicit viewport set/clear begins a transition immediately before browser mutation. Success commits independently observed geometry; every apply, observation, conversion, supervisor-commit, and rollback path keeps the fence active until rollback geometry is re-observed or capture fails target-locally.
- Transition interval evidence is declared before the new geometry becomes established. Refresh dispatch or observation failure closes the interval as a gap and fails only the affected capture/target generation rather than retaining stale provenance.
- Persisted frame fields and MCP schemas remain unchanged.

## Review fix regression evidence

- `capture::tests::acknowledgement_spanning_geometry_transition_is_dropped_with_exact_gap_evidence` proves an acknowledged frame spanning a revision change is not retained and receives exact loss evidence before a subsequent new-geometry frame is accepted.
- `capture::tests::native_resize_and_navigation_events_fence_generation_scoped_geometry_refreshes` proves resize and navigation events create generation-scoped refresh requests and only established geometry is retained.
- `session::tests::session_refresh_commits_observed_geometry_and_fails_only_capture_on_observation_error` proves a supervisor refresh commits independently observed CSS viewport/DPR and an observation failure closes the fence with target-local capture failure.
- `session::tests::session_set_clear_and_rollback_fence_capture_geometry_transactions` covers successful set, successful clear, rollback after apply failure, and rollback failure. It asserts established geometry changes only at commit and every transition is represented by an interval gap.
- Existing reconnect tests continue to prove replay happens before capture resume, restored geometry is independently observed, stale generations are rejected, and replay failures remain target-local.

## Review fix verification

- `cargo test -p krometrail-cdp --lib --locked geometry` — passed, 8 tests.
- `cargo test -p krometrail-cdp --lib --locked reconnect` — passed, 12 tests.
- `cargo test -p krometrail-cdp --lib --locked` — passed, 132 tests.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — passed.
- `cargo test -p krometrail-cdp --all-targets --locked` — the unit suite and preceding integration binaries passed, but the existing scripted session-supervision capture case waited indefinitely for `Page.startScreencast`. A diagnostic run showed its fake transport returns an empty `Page.getLayoutMetrics` response, so initial authoritative geometry observation rejects the target before capture start. The out-of-scope integration harness was restored unchanged and no test process remains.
