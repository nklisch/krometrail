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
- **Capture recovery**: geometry-refresh failure leaves the fence open without adopting new geometry. Capture stays active, but every crossing frame is acknowledged and declared as a screencast-paused gap until a later geometry event successfully refreshes the same transition.
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

Only an authoritative `commit_geometry_transition` completes a transition. Retry exhaustion and dispatch loss retain that transition and its geometry fence, so later browser geometry events can redispatch it without treating a previously established geometry as current evidence. Keep logs/counters truthful.

**Acceptance criteria**:

- [ ] Exhausted geometry refresh retries do not set capture state to failed.
- [ ] Frames crossing the transition are acknowledged and recorded as gaps, not persisted with uncertain geometry.
- [ ] Subsequent frames remain fenced and are declared as gaps until a later authoritative refresh adopts new geometry.
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

Using declared desktop geometry for capture could diverge from screencast payload pixels. `RawFrame::after_ack` remains a malformed-envelope guard, not geometry authority: no frame payload can clear an unresolved geometry fence. A malformed frame is rejected explicitly rather than silently resized.

## Implementation summary

- Desktop responsive acknowledgement now validates exact declared `cssLayoutViewport` dimensions while continuing to report the observed `cssVisualViewport`; mobile acknowledgement remains visual-viewport exact.
- Capture geometry for an acknowledged desktop override uses declared layout dimensions, while raw frame-envelope rejection remains terminal.
- Geometry-refresh exhaustion and refresh-dispatch loss keep the transition fenced. Crossing frames are acknowledged and declared as bounded `ScreencastPaused` gaps; they never inherit stale geometry provenance or invent a `frame_envelope` capture failure.
- A later browser geometry event redispatches the existing transition, and only an authoritative geometry read can commit a replacement. A separately failed target still stops its own stream without misreporting a frame-envelope failure.

## Verification

- `cargo test -p krometrail-cdp --all-targets --locked`
- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings`

## Review findings

Standard review requested changes before approval:

- A geometry-refresh retry exhaustion must leave the geometry fence active. Keeping the last established dimensions as evidence provenance after a resize, navigation, or zoom event is unsafe; acknowledged frames remain dropped as paused gaps until a later authoritative refresh commits replacement geometry.
- Desktop viewport guidance and the stable specification must name declared layout geometry as the acknowledgement authority and visual content as a separate observation. Mobile retains visual-viewport acknowledgement semantics.

This is the only corrective pass for these findings. Fix verification must prove both conditions before the feature returns to `done`; no additional independent review is required.
