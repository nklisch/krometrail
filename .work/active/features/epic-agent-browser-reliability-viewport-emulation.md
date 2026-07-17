---
id: epic-agent-browser-reliability-viewport-emulation
kind: feature
stage: implementing
tags: [browser, agent-ux, visual]
parent: epic-agent-browser-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Target-scoped viewport emulation

## Brief

Resolve GitHub issue #10 with an additive browser-control operation that applies or clears explicit viewport/device metrics on one selected target and reports the effective CSS viewport, device scale, mobile layout, and touch state. The override must survive ordinary navigation, be restored or explicitly cleared across target attachment lifecycle, and avoid opaque named-device presets in the first stable contract.

Viewport changes during recording must remain honest in source-frame metadata and artifact normalization. The operation returns live evidence under the same outcome rules as other state-changing controls and records a correlation marker suitable for later temporal analysis.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: independent public capability consumed by final MCP schema and skill guidance.

## Simplification opportunity
- Establish one explicit viewport authority rather than adding both launch-only sizing and a separate runtime preset system.

## Foundation references
- `docs/SPEC.md` — viewport/output and browser-control contracts
- `docs/ARCHITECTURE.md` — target-scoped control state and reconnect restoration
- `docs/VISUAL-EVIDENCE.md` — frame geometry and normalization

## Design decisions

- **Public operation**: add one registry-declared `set_viewport` page operation with an explicit
  tagged `override` or `clear` mode. It defaults to the selected page and returns both the declared
  override state and independently observed CSS viewport/device/touch facts.
- **First contract boundary**: accept explicit width, height, device-scale factor, mobile-layout,
  and touch booleans. Do not add named device presets, launch-only sizing, orientation aliases, or
  user-agent override yet; those couple unrelated identity policy to a capability whose immediate
  requirement is responsive layout geometry.
- **Lifecycle authority**: retain the requested override in the existing single-writer supervisor
  target state. On a new attachment generation, restore it before session domains, visibility, or
  capture start/resume. Ordinary same-target navigation relies on Chrome's persistent emulation
  state and does not reissue the command.
- **Failure semantics**: applying metrics and touch is transactional at the adapter boundary. If a
  later command or effective-state observation fails, best-effort restore the previous override and
  report a structured target operation failure. Reconnect restoration failure fails that target
  before capture starts rather than silently recording under native geometry.
- **Temporal evidence**: represent the change as a normal state-changing page-operation anchor.
  Source frames retain their own dimensions/scale metadata, and incompatible geometry divides
  artifact epochs under the existing visual-evidence contract; no capture restart or inferred
  rescaling is introduced.
- **UI alignment**: no product UI surface is introduced; this is an MCP/domain operation, so no
  mockup fallback applies.

## Architectural options

### Option A: Resize the native Chrome window

Use `Browser.getWindowForTarget` and `Browser.setWindowBounds`. This matches visible desktop window
size but cannot express device scale, mobile viewport semantics, or touch; window-manager behavior
also produced the original failure. Rejected.

### Option B: Target-scoped CDP emulation with supervised restoration (chosen)

Use `Emulation.setDeviceMetricsOverride`, `Emulation.setTouchEmulationEnabled`, and their clear
counterparts on the attached flat session. Store only the validated override in supervisor state
and restore it before capture on reattachment. Chosen because it is explicit, target-scoped,
testable through the existing CDP seam, and consistent with reconnect/capture ownership.

### Option C: Named device catalog plus user-agent/network presets

Expose devices such as “iPhone” and fan each preset into metrics, touch, user agent, and perhaps
network conditions. This is convenient but creates a versioned catalog and silently bundles
unrelated behavior before the explicit metrics contract is proven. Deferred.

## Implementation Units

### Unit 1: Validated viewport domain contract and registry operation

**Files**:
- `crates/krometrail-core/src/browser/viewport.rs` (new)
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/control.rs`
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-core/src/browser/batch.rs`
- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-mcp/src/registry.rs`
- `crates/krometrail-mcp/src/response.rs`

**Story**: `epic-agent-browser-reliability-viewport-emulation-public-contract`

```rust
pub const MAX_VIEWPORT_DIMENSION: u32 = 10_000;
pub const MAX_VIEWPORT_DEVICE_SCALE: f64 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewportMetrics {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub device_scale_factor: DeviceScaleFactor,
    pub mobile: bool,
    pub touch: bool,
}

impl ViewportMetrics {
    pub fn new(
        width: u32,
        height: u32,
        device_scale_factor: f64,
        mobile: bool,
        touch: bool,
    ) -> Result<Self>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", content = "metrics", rename_all = "snake_case")]
pub enum ViewportOverride { Override(ViewportMetrics), Clear }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetViewportRequest {
    #[serde(default)]
    pub target: PageSelection,
    pub viewport: ViewportOverride,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectiveViewport {
    pub css_size: CssSize,
    pub device_scale_factor: DeviceScaleFactor,
    pub mobile: bool,
    pub touch: bool,
    pub override_active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportOperationResult {
    pub operation: PageOperationResult,
    pub effective: ObservationPart<EffectiveViewport>,
}
```

`ViewportMetrics` uses constructor-backed deserialization and delegates schema to its wire shape.
Dimensions are 1..=10,000 CSS pixels and scale is finite, positive, and <=8. The existing
`DeviceScaleFactor` invariant excludes NaN and non-positive values, so add a justified manual `Eq`
implementation and retain one shared scale type rather than introducing an emulation-only float
wrapper.

Add `PageChange::ViewportConfigured { override_active: bool }` and register `SetViewport` as the
25th state-changing, page-scoped, batchable control operation. `ViewportOperationResult` reuses the
normal page-operation anchor/outcome/live observation and adds the effective metrics projection;
MCP response projection includes any live screenshot/resource links exactly like other page
mutations. Extend every exhaustive result/batch/evidence match from the registry-derived variant,
and persist the operation anchor without minting a navigation ID.

**Acceptance criteria**:
- [ ] The generated `set_viewport` schema accepts selected-target omission plus explicit override
      and clear modes, rejects zero/oversized/non-finite metrics, and remains registry-derived.
- [ ] Existing 24 operation names and request shapes remain unchanged; the additive operation is
      batchable and carries a normal timeline anchor.
- [ ] A success response distinguishes declared override state from observed CSS size, device
      scale, and touch state, while observation failure remains explicit rather than rewriting the
      already-known mutation outcome.
- [ ] No preset, user-agent, network, or launch-size authority is introduced.

### Unit 2: Transactional CDP application and effective-state observation

**Files**:
- `crates/krometrail-cdp/src/control/viewport.rs` (new)
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/session/operations.rs`
- `crates/krometrail-cdp/src/session/evidence.rs`
- `crates/krometrail-cdp/src/control/batch.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/verified_interactions.rs`

**Story**: `epic-agent-browser-reliability-viewport-emulation-public-contract`

```rust
impl PageControl {
    pub(crate) async fn set_viewport(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: SetViewportRequest,
        previous: Option<ViewportMetrics>,
        cancel: &OperationCancellation,
    ) -> Result<ViewportOperationResult>;
}

async fn apply_viewport(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    viewport: Option<ViewportMetrics>,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>;

async fn observe_effective_viewport(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    declared: Option<ViewportMetrics>,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<EffectiveViewport>;
```

Override mode sends `Emulation.setDeviceMetricsOverride` followed by
`Emulation.setTouchEmulationEnabled` with bounded integer metrics. Clear mode disables touch and
calls `Emulation.clearDeviceMetricsOverride`. If command two fails, call the same helper with the
previous state before returning the sanitized adapter error. After acknowledgement, read
`Page.getLayoutMetrics` plus a side-effect-free `Runtime.evaluate` of
`innerWidth`, `innerHeight`, `devicePixelRatio`, and `navigator.maxTouchPoints`; compare observed
CSS dimensions/scale/touch to the requested state within the existing finite CSS conventions.
`mobile` is the acknowledged CDP layout mode because Chrome exposes no independent page property
for that flag. Clear reports native observed size/scale and false mobile/touch.

Allocate the interaction anchor before dispatch. Commit supervisor state only after command and
effective-state validation succeed; then collect the ordinary post-operation live observation. A
failed post-operation screenshot/snapshot is degraded evidence, not proof the viewport command
failed.

**Acceptance criteria**:
- [ ] Desktop and mobile-sized overrides produce the requested CSS viewport, scale, mobile flag,
      and touch state on the selected target while another target remains unchanged.
- [ ] Clear restores native observed metrics and disables touch without opening a new browser
      session.
- [ ] A partial CDP failure attempts to restore the exact prior override and does not commit a new
      supervisor state or success anchor.
- [ ] Navigation on the same attachment preserves the override and returns observations at the
      overridden geometry.

### Unit 3: Single-writer lifecycle restoration before capture

**Files**:
- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`
- `crates/krometrail-cdp/src/session/operations.rs`
- `crates/krometrail-cdp/tests/target_reducer.rs`
- `crates/krometrail-cdp/tests/session_supervision.rs`

**Story**: `epic-agent-browser-reliability-viewport-emulation-restoration-and-evidence`

```rust
pub struct SupervisorTargetState {
    // existing fields
    pub viewport_override: Option<ViewportMetrics>,
}

pub enum SupervisorInput {
    // existing variants
    ViewportOverrideApplied {
        target_key: String,
        viewport: Option<ViewportMetrics>,
    },
}

pub enum SupervisorEffect {
    // existing variants
    RestoreViewport {
        context: ViewportEffectContext,
        viewport: ViewportMetrics,
    },
}

pub struct ViewportEffectContext {
    pub target_id: TargetId,
    pub connection_generation: u64,
    pub attachment_generation: u64,
    pub transport_session: TransportSessionId,
}
```

The operation applies CDP first, then reduces `ViewportOverrideApplied` to persist the acknowledged
state without reissuing it. `attach` emits `RestoreViewport` for a retained override before
`RestoreSessionDomains`, visibility probing, and capture start/resume effects. New targets and
cleared targets carry `None`. Reconnect reconciliation preserves overrides only for the same exact
browser target key; destroyed/recreated targets receive native defaults.

`apply_effects` handles restore failure by feeding the existing target-attach-failed path, making
the target failed and preventing capture under a geometry that contradicts its state. Logs contain
target/correlation IDs and dimensions/flags but never page content or URLs.

**Acceptance criteria**:
- [ ] Reducer tests prove restore effect order precedes capture start/resume for a new attachment
      generation and that cleared/new targets emit no restore.
- [ ] Reconnect to the same target key reapplies the exact metrics before domain/capture effects;
      restore failure fails only that target with actionable diagnostics.
- [ ] Target close removes the override with the target, and a new target does not inherit it.
- [ ] The same-target navigation path performs no redundant emulation command.

### Unit 4: Geometry transition evidence and public guidance

**Files**:
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/tests/session_capture.rs`
- `crates/temporal-vision/src/sequence.rs`
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/VISUAL-EVIDENCE.md`
- `docs/EVALUATION.md`
- `docs/guide/using-krometrail.md`
- `plugin/skills/krometrail/SKILL.md`

Keep capture running across the operation. Validate that screencast metadata before and after the
change retains each frame's own dimensions and scale/warnings and that the interaction anchor lies
at the geometry transition. Feed incompatible dimensions/device scale into the existing visual
epoch split; never silently stretch, discard, or label old frames with the current override. If the
current sequence boundary does not compare device scale as well as dimensions, extend that one
authority rather than adding viewport-specific artifact branches.

Update foundation/runtime/skill guidance with explicit override and clear examples only after the
operation exists. Explain that a viewport change creates visual epochs and agents should query each
epoch or request declared normalization. The later agent-contract feature may economize examples,
but this implementation owns removal of any false claim that responsive sizing requires external
window automation.

**Acceptance criteria**:
- [ ] Stored frames on both sides of a viewport change preserve their own source geometry and known
      metadata warnings; capture ordinals remain continuous and no artificial gap/restart appears.
- [ ] Artifact input splits incompatible viewport/scale epochs unless normalization is explicit,
      matching `docs/VISUAL-EVIDENCE.md`.
- [ ] The timeline contains the `set_viewport` interaction anchor so agents can query around the
      transition without calculating timestamps.
- [ ] Skill/docs examples cover override, clear, target scoping, effective metrics, and epoch
      interpretation; generated docs are regenerated, not hand-edited.

## Implementation Order

1. `epic-agent-browser-reliability-viewport-emulation-public-contract` — core validation,
   registry/schema/result projection, transactional target command, and effective observation.
2. `epic-agent-browser-reliability-viewport-emulation-restoration-and-evidence` — supervisor state,
   reconnect ordering, capture/epoch verification, foundation docs, and skill guidance.
3. Run core/schema/unit tests, scripted reducer/supervision/capture tests, the full Rust gate, and
   opt-in real-Chrome responsive qualification on Linux and macOS.

## Simplification

- One `set_viewport` operation replaces both launch-only sizing and external window automation;
  do not add parallel viewport configuration sources.
- Reuse `DeviceScaleFactor`, `PageOperationResult`, operation registry generation, supervisor
  attachment generations, and temporal epoch validation rather than introducing duplicate wire,
  marker, reconnect, or artifact registries.
- Store only the requested active override on each target. Native effective metrics are observed on
  clear and do not become a second retained configuration authority.
- Avoid a device catalog, user-agent override, orientation enum, and capture restart until a real
  use case requires those independently reviewable contracts.

## Testing

- Core tests protect bounded metrics, constructor-backed serde/schema parity, selected-page
  default, additive registry completeness, and result invariants.
- Scripted CDP tests protect command parameters/order, partial-failure rollback, effective-state
  mismatch handling, target isolation, and clear behavior.
- Pure reducer tests protect retention and exact restore-before-capture ordering; supervision tests
  protect same-key reconnect restoration and target-local failure isolation.
- Capture/temporal tests protect continuous ordinals, per-frame geometry, and epoch splitting.
- Opt-in real-Chrome `verified_interactions` qualification proves responsive CSS media behavior,
  devicePixelRatio, touch points, navigation persistence, target isolation, and native clear on
  both supported operating systems. Do not make Chrome mandatory for default workspace tests.

## Risks

- **Riskiest assumption**: CDP acknowledges mobile layout mode without a separate observable
  property. The result labels it as acknowledged declared state while dimensions, scale, and touch
  are independently observed; responsive media-query behavior in real Chrome is the end-to-end
  qualification.
- Applying metrics and touch uses two CDP commands. Best-effort rollback plus commit-after-verify
  prevents Krometrail state from claiming a failed partial update, but a rollback transport failure
  must fail the target and surface diagnostics rather than continue ambiguously.
- Reconnect effect order is load-bearing: capture must not resume first. Reducer order tests and a
  scripted reconnect test guard this boundary.
- Large overrides can increase browser/capture load. Explicit finite bounds prevent unbounded
  dimensions, while existing capture maximums, queue loss accounting, and status remain the
  performance authority.

## Advisory review

This additive stable MCP and reconnect/capture design warrants independent scrutiny, but the
delegated design boundary explicitly prohibits nested agents. Advisory review is deferred to the
parent epic's required fresh-context feature/aggregate review; this does not block the design stage
transition.
