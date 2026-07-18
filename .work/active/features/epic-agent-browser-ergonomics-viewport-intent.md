---
id: epic-agent-browser-ergonomics-viewport-intent
kind: feature
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Viewport intent and presets

## Brief

Add intention-revealing responsive-CSS and mobile-device presets that materialize into the existing explicit target-scoped viewport override. Return preset/intent provenance and warn when observed layout geometry differs materially from the requested visual viewport, especially when missing page viewport metadata produces Chrome's 980px mobile layout.

Custom metrics and clear retain their stable meanings. Presets do not change user agent, browser identity, or the lifecycle-complete reconnect/rollback authority.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent additive viewport contract

## Simplification opportunity

Materialize all presets through `ViewportMetrics` and derive guidance from the already observed `PageState`; do not add a parallel emulation state machine.

## Foundation references

- `docs/SPEC.md` — Viewport emulation
- `docs/ARCHITECTURE.md` — Target Lifecycle

## Design decisions

- **Preset set**: publish five browser-agnostic presets—`responsive_small`, `responsive_tablet`, `responsive_desktop`, `mobile_phone`, and `mobile_tablet`—rather than vendor device names that imply user-agent or hardware fidelity.
- **Wire compatibility**: reshape the private Rust enum to struct variants while preserving existing JSON exactly (`{"mode":"override","metrics":...}` and `{"mode":"clear"}`), then add `{"mode":"preset","preset":"..."}` as a third additive variant.
- **Lifecycle authority**: materialize a preset into `ViewportMetrics` before CDP dispatch and store only those explicit metrics in supervisor/reconnect state; preset and intent are response provenance, not a parallel persisted emulation state.
- **Mismatch threshold**: guidance is emitted when either layout dimension differs from the visual viewport by more than `max(8 CSS px, 5%)`; a mobile preset with no viewport metadata and a layout width at least 1.5 times the visual width receives the specific `likely_missing_viewport_metadata` code.
- **No implicit UA claim**: every materialization reports `user_agent_emulated: false`; this is provenance rather than a warning because it is always true for this contract.

## Architectural choice

Three approaches were considered. A separate device-emulation subsystem could grow toward full phones but would duplicate reconnect and rollback state and falsely imply user-agent fidelity. Client-side skill-only presets would avoid runtime code but could drift and would not return authoritative provenance. The selected approach adds one preset variant at the validated core boundary, materializes it immediately into existing `ViewportMetrics`, and derives guidance from independently observed geometry. It preserves the lifecycle-complete target override as the only mutable authority.

The trickiest unit is distinguishing successful emulation from surprising page layout. Chrome can correctly acknowledge a 390×844 mobile visual viewport while laying out a page at 980 CSS pixels because the page lacks viewport metadata. The adapter must therefore retain strict acknowledgement against the visual viewport, independently decode the CSS layout viewport and a boolean viewport-meta fact, and return guidance without treating valid Chrome behavior as failure or silently changing it.

## Implementation Units

### Unit 1: Additive preset, intent, and guidance domain contract

**Files**: `crates/krometrail-core/src/browser/viewport.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/browser/operation.rs`

**Story**: `epic-agent-browser-ergonomics-viewport-intent-contract`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportPreset {
    ResponsiveSmall,
    ResponsiveTablet,
    ResponsiveDesktop,
    MobilePhone,
    MobileTablet,
}

impl ViewportPreset {
    pub fn materialize(self) -> ViewportMetrics;
    pub const fn intent(self) -> ViewportIntent;
}

// Exact preset table:
// responsive_small   => 390×844,  DPR 1, mobile false, touch false
// responsive_tablet  => 768×1024, DPR 1, mobile false, touch false
// responsive_desktop => 1440×900, DPR 1, mobile false, touch false
// mobile_phone       => 390×844,  DPR 3, mobile true,  touch true
// mobile_tablet      => 768×1024, DPR 2, mobile true,  touch true

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportIntent { BrowserDefault, Custom, ResponsiveCss, MobileDevice }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ViewportOverride {
    Override { metrics: ViewportMetrics },
    Preset { preset: ViewportPreset },
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewportMaterialization {
    pub intent: ViewportIntent,
    pub preset: Option<ViewportPreset>,
    pub metrics: Option<ViewportMetrics>,
    pub user_agent_emulated: bool,
}

impl ViewportOverride {
    pub fn materialize(self) -> ViewportMaterialization;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportGuidanceCode {
    LayoutViewportMismatch,
    LikelyMissingViewportMetadata,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewportGuidance {
    pub code: ViewportGuidanceCode,
    pub message: NonEmptyText,
}

pub struct EffectiveViewport {
    pub css_size: CssSize,             // existing visual viewport field
    pub layout_css_size: CssSize,      // additive independently observed field
    pub device_scale_factor: DeviceScaleFactor,
    pub mobile: bool,
    pub touch: bool,
    pub override_active: bool,
    pub viewport_meta_present: bool,   // content is never returned
}

pub struct ViewportOperationResult {
    pub operation: PageOperationResult,
    pub effective: ObservationPart<EffectiveViewport>,
    pub materialization: ViewportMaterialization,
    pub guidance: Vec<ViewportGuidance>,
}

pub fn viewport_guidance(
    materialization: ViewportMaterialization,
    effective: &EffectiveViewport,
) -> Vec<ViewportGuidance>;
```

**Implementation notes**:

- Implement manual/validated deserialization through a wire enum so the two existing 1.x request encodings remain accepted and serialize identically. The new preset variant contains only `preset`; reject `metrics` mixed with `preset` and unused fields on `clear`.
- `materialize()` is pure and exhaustive. Custom override returns `Custom`; responsive presets return `ResponsiveCss`; mobile presets return `MobileDevice`; clear returns `BrowserDefault` with no metrics. All return `user_agent_emulated: false`.
- Compare layout and visual dimensions using `abs(layout - visual) > max(8.0, visual * 0.05)`. Emit at most one guidance entry. Choose the specific missing-metadata code only when intent is `MobileDevice`, `viewport_meta_present` is false, and layout width is at least 1.5× visual width; otherwise use the general mismatch code.
- Guidance messages state observed layout and visual sizes, explain that the override was acknowledged, and for missing metadata suggest either adding page viewport metadata or using a responsive preset for CSS-breakpoint testing. They never include page content.

**Acceptance criteria**:

- [ ] Existing override and clear JSON round-trip byte-for-byte; preset JSON is additive and rejects mixed/unknown fields.
- [ ] Unit tests lock every preset's exact metrics/intent and prove the mismatch boundary immediately below, at, and above the threshold.
- [ ] Missing-metadata guidance only appears for the defined mobile/no-meta/1.5× case; clear/custom/responsive results retain truthful provenance.

### Unit 2: Materialize before the lifecycle boundary and observe layout intent

**Files**: `crates/krometrail-cdp/src/session/operations.rs`, `crates/krometrail-cdp/src/control/viewport.rs`, `crates/krometrail-cdp/src/control/tests.rs`, `crates/krometrail-cdp/src/session/mod.rs`

**Story**: `epic-agent-browser-ergonomics-viewport-intent-runtime`

```rust
pub(crate) async fn observe_effective_viewport(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    declared: Option<ViewportMetrics>,
) -> Result<EffectiveViewport>;

fn decode_effective_viewport(
    layout: &serde_json::Value,
    runtime: &serde_json::Value,
    declared: Option<ViewportMetrics>,
    target_id: TargetId,
) -> Result<EffectiveViewport>;
```

**Implementation notes**:

- In `SetViewport`, compute `ViewportMaterialization` first and pass only `materialization.metrics` through the existing apply/observe/commit/capture/rollback path. `SupervisorInput::ViewportOverrideApplied` and reconnect continue storing `Option<ViewportMetrics>` unchanged.
- Extend the existing `Page.getLayoutMetrics` decoder to read both `cssVisualViewport.clientWidth/clientHeight` and `cssLayoutViewport.clientWidth/clientHeight` with finite-positive validation.
- Extend the existing side-effect-free runtime expression to return only `devicePixelRatio`, `navigator.maxTouchPoints`, and `document.querySelector('meta[name="viewport"]') !== null`; do not return viewport metadata content.
- Preserve acknowledgement checks against requested visual size, DPR, and touch. A layout mismatch is guidance, not `target_failed`. Compute guidance after acknowledgement and include it alongside the same full live observation.
- All apply, observe, supervisor-commit, capture-transition, rollback, snapshot-invalidation, and reconnect ordering remains unchanged. A failed apply/observation still rolls back to the previous explicit metrics and returns failed operation evidence without preset provenance claiming success.

**Acceptance criteria**:

- [ ] Scripted transport tests prove preset and equivalent custom requests emit identical CDP commands and supervisor state.
- [ ] Reconnect restores exact materialized metrics without requiring preset identity; rollback restores the previous metrics after every injected failure stage.
- [ ] A valid 390px mobile visual viewport with a 980px layout viewport succeeds and returns specific guidance rather than failing or altering geometry.
- [ ] Clear still disables touch, clears device metrics, resets page scale, and reports browser-default intent with no active override.

### Unit 3: Published schema, real-browser behavior, and agent guidance

**Files**: `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-cdp/tests/viewport_override.rs`, `tests/fixtures/browser/page-observation/index.html`, `plugin/skills/krometrail/SKILL.md`

**Story**: `epic-agent-browser-ergonomics-viewport-intent-runtime`

```rust
// Existing set_viewport tool accepts:
// {"viewport":{"mode":"preset","preset":"responsive_small"}}
// and returns materialization + effective visual/layout geometry + guidance.
```

**Implementation notes**:

- Update the canonical generated schema test to assert all three modes, five preset names, and unchanged bounds for custom metrics.
- Qualify two representative presets in real Chrome: responsive-small must produce equal 390px visual/layout widths; mobile-phone against a deterministic no-meta fixture must produce a 390px visual width, approximately 980px layout width, and specific guidance. Add a viewport-meta fixture variant only if one existing fixture cannot express both states deterministically.
- Update the plugin skill to recommend responsive presets for CSS breakpoint/layout tests, mobile presets only for mobile-layout/touch behavior, custom metrics for exact bespoke geometry, and clear to restore browser defaults. Explain the no-UA guarantee and how to interpret guidance.

**Acceptance criteria**:

- [ ] MCP schema and response tests publish preset provenance/effective geometry while omitted new inputs preserve full 1.x behavior.
- [ ] Real Chrome verifies responsive and no-meta mobile behavior, exact screenshot/visual dimensions, and target isolation across a second page.
- [ ] Skill guidance prevents agents from interpreting `mobile_phone` as a full hardware/user-agent emulation or a layout mismatch as failed application.

## Implementation order

1. `epic-agent-browser-ergonomics-viewport-intent-contract` defines the additive wire shape, exact preset table, materialization, and guidance logic.
2. `epic-agent-browser-ergonomics-viewport-intent-runtime` threads materialized metrics through the existing lifecycle, observes layout facts, qualifies real Chrome, and updates MCP/skill surfaces.

## Simplification

- Keep one `ViewportMetrics` authority across custom, preset, reconnect, rollback, and capture geometry; do not persist preset identity in supervisor state.
- Reuse the current layout/runtime observation calls rather than adding page evaluation or a device descriptor registry.
- Keep the five preset constants in one exhaustive `ViewportPreset::materialize` match; generated schema derives names from the same enum.
- Retain one `set_viewport` tool and remove no existing custom/clear tests or paths.

## Testing

- Core tests protect stable wire compatibility, exact preset materialization, and pure guidance classification.
- Scripted CDP/session tests protect command ordering, state commit/rollback/reconnect, and geometry decoding.
- Two real-browser cases protect Chrome's responsive and missing-meta mobile behavior; broader device catalogs would add cost without protecting this contract.
- Existing lifecycle-complete viewport tests remain authoritative and should be parameterized for preset/custom equivalence where that reduces duplication.

## Risks

- Chrome's approximately 980px no-meta layout can vary slightly with browser revision. Classification uses the ratio and metadata fact rather than equality; the real-browser test should assert the semantic threshold, not an exact 980 constant.
- Preset names can become compatibility commitments. The deliberately small, vendor-neutral set covers the current responsive/mobile intent without implying a comprehensive device catalog.
- Additional result fields increase the stable response surface. Keep them factual, bounded, and derived from the same acknowledged operation; do not expose viewport metadata content or infer user-agent/device identity.

## Integrated implementation evidence

- Both implementation stories are done: the stable additive intent/preset/guidance contract and the lifecycle/MCP/real-browser integration.
- Existing custom and clear JSON remain exact; the additive preset mode offers five vendor-neutral choices and materializes immediately into the existing explicit metrics authority.
- Apply, observe, supervisor commit, capture transition, rollback, clear, navigation restore, and reconnect retain their established ordering and state model. Only metrics persist; preset identity remains response provenance.
- Visual and layout geometry plus viewport-meta presence are observed independently. Strict acknowledgement remains visual/DPR/touch based, while material layout divergence yields at most one bounded content-free guidance item.
- The plugin skill leads with the smallest responsive preset as the reasonable ergonomic default, expands to larger responsive surfaces, and reserves mobile/custom options for explicit intent.
- Integrated core, CDP session/decoder, MCP schema/projection, workspace all-target checks, and the bounded real-Chrome qualification passed.
- Real Chrome verified responsive-small equal geometry and exact screenshot size, mobile-phone missing-meta guidance with a wider layout viewport, navigation persistence, clear, and target isolation.
- No feature-scope blocker or adjacent finding remains; the feature is ready for independent review.

## Review record

- Effective weight: standard; pass: 1; verdict: approve after fixes.
- Findings fixed: legacy navigation/control/shared-CDP observation doubles now provide the independently required layout viewport and viewport-meta fact, protecting full-library execution rather than only focused new tests; browser-default/clear materialization with no requested metrics now returns no mismatch guidance and therefore never claims an override was acknowledged.
- Regression evidence: divergent clear geometry produces an empty guidance list, the independent viewport observation test asserts layout geometry and metadata presence, and the navigation restore plus shared scripted transport use the exact current side-effect-free runtime expression.
- Verification: core viewport tests passed (10), the complete `krometrail-cdp` library suite passed (140), and the earlier bounded real-Chrome responsive/mobile preset qualification remains green.
- Closure: accepted blockers were corrected in `15f6825`; per the standard one-pass policy, verified fixes close the feature without a second independent review.
