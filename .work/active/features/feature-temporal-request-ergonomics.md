---
id: feature-temporal-request-ergonomics
kind: feature
stage: done
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
- [x] `{"kind":"accept"}` deserializes to `Accept { prompt_text: None }`.
- [x] `{"kind":"accept","prompt_text":"x"}` deserializes with the text.
- [x] `{"kind":"dismiss"}` unchanged.
- [x] Regenerated `handle_dialog` schema reflects the flattened shape.

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
- [x] A `generate_region_filmstrip` call with only `region` (+range/handle)
      succeeds, using empty markers, black background/padding, default
      tile_limit/labels/output/display_scale, and the range's effective anchor.
- [x] Explicit values still validate exactly as before (bad anchor outside
      range still rejected; FitLimits display_scale still rejected).
- [x] A `generate_artifacts` call with only `generators` (+range/handle)
      succeeds with empty markers and the default failure policy.
- [x] Canonical checked-in schemas regenerate and match (byte/digest gate).

### Unit 3: Source-frame unlimited sentinel
**File**: `crates/krometrail-core/src/progressive.rs`
(`SourceReadLimitsRequest::new`).

Materialize `0 → ceiling` for each of the three limits before NonZero
construction; run the existing over-ceiling and item≤total checks on
materialized values. Update field doc comments.

**Acceptance**:
- [x] `max_item_bytes: 0` resolves to `MAX_SOURCE_ITEM_BYTES`; likewise
      `max_frames: 0` and `max_total_bytes: 0` to their ceilings.
- [x] A fully-zero limits triple resolves to all three ceilings and passes.
- [x] Over-ceiling explicit values still rejected with the sized message.
- [x] item>total (both explicit) still rejected.

### Unit 4: Canvas-limit diagnostic
**File**: `crates/temporal-vision/src/render/canvas.rs`

Give `canvas_limit_error` the offending dimension/limit context and emit a
message naming width/height and the `output.max_*` to raise. Update call sites
to pass the context they have.

**Acceptance**:
- [x] The error message names the overflowing dimension and the output cap to
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

## Implementation notes

- `generate_artifacts` now defaults `failure_policy` to `allow_partial`,
  matching the existing standalone/bundle policy used by the MCP registry and
  debug-bundle construction. Explicit policies remain unchanged.
- Shared artifact default constructors are reused by the progressive
  filmstrip wire instead of duplicating labels, output limits, colors, and
  scale values. Omitted filmstrip anchors are materialized from
  `ResolvedRange::resolved_anchor.effective_time`; explicit anchors still pass
  through the existing range validation.
- The repository publishes these MCP contracts from runtime-generated
  schemars values and has no checked-in `handle_dialog` or progressive-tool
  schema artifacts or blessing command. The schema generator test now covers
  the changed tool families; no parallel generated files were introduced.
- Width/height diagnostics use a contextual `canvas_output_limit_error` at the
  storyboard and region-filmstrip layout checks. Other canvas arithmetic still
  uses the generic bounded failure because those call sites do not have output
  width/height caps available.

## Review (cross-model, Fable reviewing Luna)

Verdict SHIP — no blockers/majors/minors. Verified: dialog internal-tag +
deny_unknown_fields + defaulted Option round-trips all three shapes and rejects
the old `value`-wrapper; a repo-wide grep found no other consumer of the old
`{"kind":"accept","value":{…}}` JSON shape. Filmstrip default anchor
(`resolved_anchor.effective_time`) is guaranteed inside `resolved_range` because
`ResolvedRange::validate()` enforces exactly that invariant on every range, so
the defaulted anchor can never fail the filmstrip re-check. Source-frame
`0→ceiling` is applied before all downstream checks so the `NonZero::expect`
can't panic. Canvas-diagnostic branch selection and call-site argument order
verified. No checked-in MCP tool-schema artifacts exist (generated at runtime
from schemars), so nothing needed regeneration.

- **Nit (not fixed, defensible)**: `max_item_bytes: 0` (→ 32 MB ceiling) with a
  small explicit `max_total_bytes` is rejected by the item≤total check, and the
  message reports the materialized 32 MB the caller never typed. A per-item cap
  above the aggregate cap is meaningless, so rejection is correct; only the
  message could name the sentinel. Left as-is.
