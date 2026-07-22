---
id: feature-temporal-anchor-ergonomics
kind: feature
stage: implementing
tags: [agent-ux, visual, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Temporal anchor ergonomics

## Brief

Small temporal request-shape frictions from GitHub issue #14 finding #10 and
the 2026-07-21 v1.4.0 local shakedown. (Distinct from the completed
`feature-temporal-request-ergonomics`, which covered the 2026-07-19 schema
frictions.)

1. **`query.anchor` nesting is non-obvious (issue #14).** A temporal request
   only worked once its anchor was nested under `query.anchor`; the schema and
   examples made the flatter shape appear plausible. Improve the schema
   descriptions/examples — or accept the direct shape via input
   canonicalization — so the first plausible call succeeds.
2. **`resolve_temporal_range` echoes the default implicit window when an
   explicit window was applied (local shakedown).** With an explicit
   `window: {before_ms: 1500, after_ms: 1500}`, the resolved bounds correctly
   used the explicit window, but the response echoed
   `options.implicit_interaction_window: {after_ms: 250, before_ms: 150}`.
   Arguably correct ("implicit" is the fallback label), but the echo reads as
   the applied window. Make the echoed options unambiguous about which window
   governed the resolved bounds.

The confirmed `latest_interaction` bundle failure (issue #14 finding #6,
independently reproduced in the local shakedown) is promoted from backlog as
child story `feature-temporal-anchor-ergonomics-latest-interaction-bundle`,
implementable immediately.

## Simplification opportunity

None identified; each item is a bounded clarity or canonicalization fix.

## References

- GitHub issue #14, findings 6 and 10 (anchor nesting).
- 2026-07-21 v1.4.0 shakedown (implicit-window echo; latest_interaction repro).

## Design decisions

- **Flatten the bundle request instead of documenting the nesting**: the
  friction is shape inconsistency — `resolve_temporal_range` accepts
  `{anchor, retention, capture_gaps}` flat while `temporal_debug_bundle` wraps
  the identical `TemporalQueryRequest` under `query`. One flat shape across
  both temporal entry points removes the trap. Current Contract Discipline:
  replace the shape directly, no alias accepting both (a dual schema is the
  named anti-pattern), keep `deny_unknown_fields` so a stale nested call fails
  with a clear unknown-field error.
- **Echo disambiguation is additive, not a relabel**: the echoed
  `options.implicit_interaction_window` is truthful (it is the input fallback)
  — the gap is that nothing states which window governed. `ResolvedRange`
  gains `applied_interaction_window: Option<InteractionWindow>` (Some for
  interaction-kind anchors: the explicit window when given, else the default;
  None otherwise). No field renames on the input options type.

## Architectural choice

Direct contract replacement in core with schema flow-through — both changes
are wire-contract adjustments at the domain boundary (validated-wire-contracts);
no MCP-layer shimming, no compatibility paths.

## Implementation Units

### Unit 1: Flat bundle request
**File**: `crates/krometrail-core/src/debug_bundle.rs` (+ the MCP call site
that constructs `TemporalDebugBundleRequest`)

`TemporalDebugBundleRequest` / its `Wire` twin become flat:

```rust
pub struct TemporalDebugBundleRequest {
    anchor: TemporalRangeAnchor,
    retention: RetentionPolicy,
    capture_gaps: CaptureGapPolicy,
    caller_markers: Vec<ArtifactMarker>,
    orientation: OrientationPolicy,
    epochs: BundleEpochScope,
}
```

Internally it still builds the validated `TemporalQueryRequest` exactly once
(`new` re-validates as today). `query()` accessor keeps returning the composed
`TemporalQueryRequest` so the service path is untouched.

**Implementation Notes**:
- `deny_unknown_fields` stays: a legacy `query`-nested call fails with an
  explicit unknown-field message.
- Check `docs/SPEC.md` Temporal Queries for any nested-shape example and
  align it; regenerate nothing by hand.

**Acceptance Criteria**:
- [ ] Flat wire round-trip; nested `query` input rejected with a clear error.
- [ ] Bundle service tests updated; behavior (resolution, artifacts,
      validation) unchanged.

### Unit 2: Applied-window echo
**File**: `crates/krometrail-core/src/timeline/range.rs`

`ResolvedRange` gains `pub applied_interaction_window:
Option<InteractionWindow>`, populated at the two seed sites that already
compute `window.unwrap_or(options.implicit_interaction_window)`
(`range.rs:1158`, `range.rs:1190`); `None` for non-interaction anchor kinds.
Schema description on `options.implicit_interaction_window` clarifies it is
the fallback input, with `applied_interaction_window` as the governing value.

**Acceptance Criteria**:
- [ ] Explicit window → `applied_interaction_window` echoes it exactly.
- [ ] Omitted window on an interaction anchor → echoes the default.
- [ ] Non-interaction anchors → `None`.

## Implementation Order
1. Unit 1 (bundle shape)
2. Unit 2 (echo) — independent, but landing together keeps one temporal
   contract commit.

## Simplification
- No new abstractions; one wrapper struct field removed (`query` nesting).
- No child stories — single-stride, tight cohesion (the sibling bug story
  `latest-interaction-bundle` already landed independently).

## Testing
- Wire tests per unit acceptance criteria (validated-wire-contracts).
- One bundle-service test exercising the flat request end-to-end through the
  existing store rig.
- No test removal; existing nested-shape constructions in tests migrate to
  the flat shape.

## Risks
- Low. The flat shape collides with no existing top-level bundle field
  (`anchor`/`retention`/`capture_gaps` are free at that level). Stale callers
  fail loudly, which is the intended contract behavior.
