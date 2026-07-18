---
id: truthful-screencast-geometry
kind: feature
stage: implementing
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

- [ ] Adaptive changes to screencast metadata/encoded dimensions do not change retained viewport provenance.
- [ ] An acknowledged geometry update applies viewport and DPR together to subsequent frames without restarting capture.

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

- [ ] Initial native capture and restored override capture begin with independently observed CSS viewport and DPR.
- [ ] Apply and clear update both fields atomically after success; rollback leaves prior capture geometry intact.
- [ ] Reconnect replay restores geometry before capture resumes and remains target-local on failure.

### Unit 3: Regression coverage for adaptive encoding

**Files**: `crates/krometrail-cdp/src/capture/tests.rs`, `crates/krometrail-cdp/src/session/tests.rs`

**Acceptance criteria**:

- [ ] A stream holding 600×500/DPR1 records that provenance for a 1200×1000 encoded frame followed by a 600×500 encoded frame whose screencast metadata claims 300×250.
- [ ] A real acknowledged change to 390×844/DPR3 appears on subsequent frames without a capture gap or stream restart.
- [ ] Session tests prove failed apply/rollback cannot mutate capture geometry and successful set/clear uses observed values.

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
