---
id: epic-browser-interface-hardening-runtime-reliability
kind: feature
stage: implementing
tags: [browser, visual]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Reliable Viewport and Capture State

## Brief

Restore responsive viewport presets on real managed Chrome and keep screencast capture alive on nested-frame pages. Desktop responsive overrides currently fail when the visual viewport is reduced by a scrollbar even though Chrome applied the declared emulation width. Separately, a frame-triggered geometry refresh that cannot immediately verify the effective viewport exhausts retries, fails the geometry transition, and terminates useful capture after already acknowledged frames.

Preserve the target-scoped viewport and geometry-fence architecture. Verify declared emulation with the correct CDP metrics, report observed visual content separately, isolate refresh failures as explicit gaps, and prove recovery without weakening frame acknowledgement or retained evidence truth.

## Source findings

- `idea-fix-viewport-preset-regression`
- `idea-fix-frame-envelope-capture`

## UI alignment

No UI surface; this is target-scoped CDP and capture-pipeline reliability.

## Design decisions

- **Desktop acknowledgement**: for non-mobile overrides, exact declared width/height are verified against `cssLayoutViewport`; `cssVisualViewport` remains the observed content area and may be smaller due to scrollbars.
- **Mobile acknowledgement**: preserve exact visual viewport verification because mobile emulation/page scale and viewport meta semantics intentionally control that surface.
- **Capture recovery**: geometry-refresh failure completes the fence without adopting new geometry, declares a screencast-paused gap, and keeps capture active. Later geometry events may retry.
- **Envelope rejection**: malformed frame payloads remain terminal at the existing boundary; only geometry refresh/dispatch failure becomes recoverable.

## Architectural choice

Refine the current viewport decoder and geometry-transition outcome. Do not add sleeps, JavaScript viewport mutation, or a second capture path. CDP emulation metrics remain authoritative, and the geometry fence continues preventing frames that cross unproven transitions from being persisted.

## Implementation Units

### Unit 1: Correct viewport acknowledgement geometry

**Story**: `epic-browser-interface-hardening-runtime-reliability-viewport-ack`

**File**: `crates/krometrail-cdp/src/control/viewport.rs`

```rust
fn declared_geometry_matches(expected: ViewportMetrics, effective: &EffectiveViewport) -> bool;
```

Use layout dimensions for responsive desktop overrides and visual dimensions for mobile overrides. Device scale and touch expectations remain exact within current tolerances. Capture geometry for an active declared desktop override uses the declared dimensions rather than scrollbar-reduced visual content.

**Acceptance criteria**:

- [ ] `responsive_small` succeeds when layout is 390×844 and visual width is scrollbar-reduced.
- [ ] A genuinely wrong layout/emulation size still fails transactionally.
- [ ] Mobile and clear behavior retain exact validation.

### Unit 2: Recoverable geometry refresh gaps

**Story**: `epic-browser-interface-hardening-runtime-reliability-capture-refresh`

**Files**: `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/session/runtime.rs`

```rust
pub(crate) fn abandon_geometry_transition(
    &self,
    transition: CaptureGeometryTransition,
) -> bool;
```

Complete the transition with no replacement geometry, declare the existing bounded paused gap, and leave the stream active. Rename the coordinator operation so callers cannot confuse a refresh failure with terminal capture failure. Keep logs/counters truthful.

**Acceptance criteria**:

- [ ] Exhausted geometry refresh retries do not set capture state to failed.
- [ ] Frames crossing the transition are acknowledged and recorded as gaps, not persisted with uncertain geometry.
- [ ] Subsequent frames use the last established geometry and a later refresh can adopt new geometry.
- [ ] W3Schools-style nested frames no longer permanently stop capture at `frame_envelope`.

## Implementation Order

1. Correct viewport acknowledgement and capture geometry derivation.
2. Make geometry-refresh failure recoverable using the proven geometry fence.

## Simplification

- Separate declared emulation acknowledgement from observed visual content in one helper.
- Rename the misleading terminal `fail_geometry_transition` API and remove the unconditional `runtime.fail(FrameEnvelope)` for refresh failures.

## Testing

- Viewport decoder tests cover scrollbar-reduced desktop, true mismatch, mobile, and clear paths.
- Capture pipeline tests prove gap declaration, active state, last-geometry continuity, and later transition success.
- Session runtime tests prove retry exhaustion calls the recoverable coordinator outcome.
- Real managed-Chrome qualification repeats responsive_small and nested-frame capture.

## Risks

Using declared desktop geometry for capture could diverge from screencast payload pixels. Existing `RawFrame::after_ack` dimension validation remains the guard. If Chrome reports a truly different envelope, that frame is rejected explicitly rather than silently resized.
