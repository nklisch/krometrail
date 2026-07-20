---
id: feature-temporal-request-ergonomics
kind: feature
stage: implementing
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Temporal and dialog request ergonomics

## Brief

Four schema frictions surfaced in the 2026-07-19 fourth shakedown (v1.2.4
live). Each forces avoidable validation round-trips or rejects a reasonable
minimal call. All are input-canonicalization / wire-contract adjustments under
Current Contract Discipline (agent tool, no third-party consumers, single
current shape — change the shape directly, no compatibility aliases). None
weakens a real invariant; each materializes a convenience input into the
existing explicit authority (ergonomic-input-canonicalization) or improves a
diagnostic.

1. **`handle_dialog` accept requires a `value` wrapper.** A bare
   `{"kind":"accept"}` is rejected; the caller must send
   `{"kind":"accept","value":{"prompt_text":null}}` even for a `confirm()` with
   no prompt.
2. **`generate_region_filmstrip` / `generate_artifacts` require presentation
   fields with no defaults.** The standalone tools reject a minimal call for
   missing `markers`, then `anchor`, then `background`, then `padding` — one
   round-trip each — though every one has a sensible default.
3. **`list_source_frames` / `fetch_source_frames` limits reject `0`.**
   `max_item_bytes: 0` (a natural "unlimited") errors "must be non-zero";
   there is no unlimited sentinel, so the caller must know the exact ceiling.
4. **`generate_region_filmstrip` canvas-limit error is opaque.** "artifact
   raster exceeds configured layout or canvas limits" names neither the
   overflowing dimension nor the fix (raise `max_width`/`max_height`).

## Design decisions
- **Dialog accept**: make `prompt_text` optional at parse time so a bare accept
  works. Switch `DialogAction` from adjacently tagged
  (`tag="kind", content="value"`) to internally tagged (`tag="kind"`) with
  `#[serde(default)] prompt_text`, giving `{"kind":"accept"}` and
  `{"kind":"accept","prompt_text":"..."}`. Wire shape changes (prompt_text
  flattens up beside kind); acceptable under Current Contract Discipline. Keep
  `Dismiss` a unit variant.
- **Filmstrip/artifacts defaults**: add `#[serde(default)]` (with matching
  `#[schemars(default)]` where schemars needs it) to the wire fields that carry
  a sound default — `markers` (empty), `background`/`padding` (black),
  `tile_limit`, `labels`, `output`, `display_scale`. `display_scale` defaults
  to `Identity` (never `FitLimits`, which validation already forbids for
  filmstrip). `anchor` defaults to the range's resolved effective anchor time
  when omitted (it must lie in the resolved range; the resolver already knows
  it), so a minimal filmstrip call needs only `region` (+ `range`/handle).
  Validation stays exactly as strict for explicitly-supplied values.
- **Source-frame limits unlimited sentinel**: treat `0` on `max_frames`,
  `max_item_bytes`, `max_total_bytes` as "use the configured ceiling"
  (`MAX_SOURCE_READ_FRAMES` / `MAX_SOURCE_ITEM_BYTES` / `MAX_SOURCE_TOTAL_BYTES`)
  rather than rejecting. Materialize `0 → ceiling` before the NonZero
  construction; the over-ceiling checks and the item≤total check run on the
  materialized values. Update the field doc comments to state the sentinel.
- **Canvas-limit diagnostic**: thread the offending dimension(s) and the
  resolved output caps into `canvas_limit_error` so the message says which of
  width/height overflowed and to raise the corresponding `output.max_*`. Keep
  the `ResourceLimitExceeded` code.

## Implementation Units

### Unit 1: Optional dialog prompt
**Files**: `crates/krometrail-core/src/browser/interaction.rs` (DialogAction),
plus any generated schema artifact for `handle_dialog`.

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogAction {
    Accept { #[serde(default)] prompt_text: Option<NonEmptyText> },
    Dismiss,
}
```

**Acceptance**:
- [ ] `{"kind":"accept"}` deserializes to `Accept { prompt_text: None }`.
- [ ] `{"kind":"accept","prompt_text":"x"}` deserializes with the text.
- [ ] `{"kind":"dismiss"}` unchanged.
- [ ] Regenerated `handle_dialog` schema reflects the flattened shape.

### Unit 2: Filmstrip & artifacts presentation defaults
**File**: `crates/krometrail-core/src/progressive.rs`
(`RegionFilmstripEvidenceRequestWire`), `crates/krometrail-core/src/artifacts.rs`
(`ArtifactGenerationRequest*Wire`).

Add serde/schemars defaults to the presentation fields; make `anchor`
`Option<SessionTime>` defaulting to the range's effective anchor at construction.
`markers` and `failure_policy` on the artifacts wire default (empty / a chosen
default policy — confirm the intended default is `require_all` vs
`allow_partial` from existing standalone defaults).

**Acceptance**:
- [ ] A `generate_region_filmstrip` call with only `region` (+range/handle)
      succeeds, using empty markers, black background/padding, default
      tile_limit/labels/output/display_scale, and the range's effective anchor.
- [ ] Explicit values still validate exactly as before (bad anchor outside
      range still rejected; FitLimits display_scale still rejected).
- [ ] A `generate_artifacts` call with only `generators` (+range/handle)
      succeeds with empty markers and the default failure policy.
- [ ] Canonical checked-in schemas regenerate and match (byte/digest gate).

### Unit 3: Source-frame unlimited sentinel
**File**: `crates/krometrail-core/src/progressive.rs`
(`SourceReadLimitsRequest::new`).

Materialize `0 → ceiling` for each of the three limits before NonZero
construction; run the existing over-ceiling and item≤total checks on
materialized values. Update field doc comments.

**Acceptance**:
- [ ] `max_item_bytes: 0` resolves to `MAX_SOURCE_ITEM_BYTES`; likewise
      `max_frames: 0` and `max_total_bytes: 0` to their ceilings.
- [ ] A fully-zero limits triple resolves to all three ceilings and passes.
- [ ] Over-ceiling explicit values still rejected with the sized message.
- [ ] item>total (both explicit) still rejected.

### Unit 4: Canvas-limit diagnostic
**File**: `crates/temporal-vision/src/render/canvas.rs`

Give `canvas_limit_error` the offending dimension/limit context and emit a
message naming width/height and the `output.max_*` to raise. Update call sites
to pass the context they have.

**Acceptance**:
- [ ] The error message names the overflowing dimension and the output cap to
      increase; code stays `ResourceLimitExceeded`.

## Implementation Order
1. Unit 1 (independent)
2. Unit 3 (independent)
3. Unit 2 (schema regen)
4. Unit 4 (independent)

## Testing
- Core unit tests per unit (deserialize doubles for dialog/limits/filmstrip
  defaults; a canvas-limit unit asserting the named-dimension message).
- Regenerate and verify canonical JSON/schema artifacts
  (canonical-json-schema-artifacts): `handle_dialog`, `generate_artifacts`,
  `generate_region_filmstrip`, `list_source_frames`, `fetch_source_frames`.
- No new real-Chrome tests; all are pure wire/validation/render-math.

## Risks
- Internal vs adjacent tagging for `DialogAction`: confirm nothing else
  pattern-matches the wire JSON shape (`value` wrapper) outside the generated
  schema; grep before landing.
- `anchor` default must equal what the resolver treats as the range's effective
  anchor so an omitted anchor and an explicit range-anchor behave identically.

Origin: 2026-07-19 fourth shakedown friction report (batch bug is a separate
feature: feature-batch-step-projection-parity).
