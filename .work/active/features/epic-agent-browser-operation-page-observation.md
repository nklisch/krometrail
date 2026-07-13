---
id: epic-agent-browser-operation-page-observation
kind: feature
stage: review
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Structured Page Observation

## Brief

Give agents a trustworthy current-page view: a compact accessibility-centered snapshot, generation-scoped actionable references, current URL/title/viewport/navigation state, and requested viewport, full-page, element, or region screenshots. The feature establishes the shared live-observation result used after state-changing operations while preserving screenshot and snapshot provenance to the selected target.

Resolve references through snapshot-local accessibility and DOM metadata, re-check backing-node validity and actionability at use time, and fail stale references with concrete refresh guidance. Explicit CSS selectors and declared coordinate spaces remain weaker escape hatches; JavaScript evaluation and read-only inspection belong here, while input dispatch, navigation, waiting, batching, and MCP registration do not.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: foundation capability — page lifecycle and interaction both consume its snapshot, reference, screenshot, and live-observation contracts
- Inherited decisions: core owns public control contracts; every state-changing operation must return honest post-action evidence; no traditional visual UI or mockups are required

## Simplification opportunity

- Replace the deferred `SnapshotGeneration` and `NodeReference` placeholders with one generation registry and one resolver. Accessibility, DOM geometry, selectors, and coordinates are evidence or fallback target forms, not competing element-identity systems.

## Foundation references

- `docs/VISION.md` — Core Experience
- `docs/SPEC.md` — Current-State Observation and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Structured Snapshots and References, MCP Boundary, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — source-image coordinate and provenance distinctions reused by screenshot metadata
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Design decisions

- **Dispatch:** Direct local probes only — the caller prohibited subagents and peer review, and the current core ports, production supervisor, raw transport seam, tests, and reserved MCP crate resolve the design without another discovery path.
- **Operation source of truth:** Add one macro-backed `BROWSER_OPERATION_REGISTRY` in core. It generates operation kind, request, result, stable name, and read-only/state-changing metadata from one declaration. This feature seeds inspection operations; later lifecycle, interaction, wait, and batch features extend that declaration rather than creating parallel enums or MCP-only schemas.
- **Session ownership:** Execute observation operations through the existing `BrowserSessionPort` and its single-writer production supervisor. The request names the Krometrail target; the actor resolves the current exact flat session and attachment generation at dispatch. No second target/session manager or direct transport handle escapes the adapter.
- **Reference lifetime:** A `NodeReference` contains target, snapshot generation, and snapshot-local node id. The CDP adapter keeps exactly one active generation per target. A newer snapshot, navigation/document change, target reattachment, or target closure invalidates the prior generation; backing-node replacement or loss of actionability fails at use time. References are deliberately not stable across snapshots.
- **Document invalidation:** Store the main-frame id and loader id observed with each snapshot and re-read `Page.getFrameTree` before resolving a reference. This gives synchronous navigation invalidation without adding another unbounded event subscription. Attachment generation and backing-node checks cover reconnect and DOM replacement.
- **Evaluation posture:** `evaluate_page` is read-only inspection. It always uses `returnByValue: true`, `throwOnSideEffect: true`, an adapter-owned timeout, and a bounded serialized result. Expressions that may mutate, return remote handles, exceed the bound, or throw fail explicitly. A future state-changing evaluation operation must be registered separately and return live evidence.
- **Live-observation degradation:** Target selection/binding failure returns an operation error. Once a target is bound, page state, snapshot, and viewport screenshot are attempted independently and returned as `ObservationPart<T>` values. A failed part carries the stable structured error and recovery action; it never becomes absent data or guessed success.
- **Screenshot payload boundary:** Core owns validated request and metadata contracts plus an in-process `EncodedScreenshot` byte payload, mirroring `EncodedFrame`. MCP later maps bytes to inline context images or persisted resources; base64 and MCP resource identifiers do not enter core.
- **Coordinate contract:** Public regions use CSS pixels and must declare `viewport_css` or `document_css`. Viewport coordinates are converted using current layout viewport offsets. Screenshot metadata reports the requested target, resolved document-space rectangle, image dimensions, device scale, and target/attachment/timing context; clipping or clamping is never silent.
- **Snapshot compactness:** Preserve a deterministic preorder of useful, non-ignored accessible nodes. Keep role, name, value, description, selected accessibility properties, parent/depth, and an actionable hint; assign references only to nodes with backing DOM metadata that appear actionable. Adapter-owned node/text limits produce an explicit truncation warning and omitted-node count.
- **UI surface:** This is an agent/API control surface, not a human screen or journey. No mockups apply.

## Architectural choice

### Option A — registry-driven, session-scoped operation executor (chosen)

Core declares one generated operation request/result registry and the infrastructure-free observation values. `BrowserSessionPort::execute` sends requests into the existing single-writer session actor. A private `krometrail-cdp::control` module translates the registered operation to raw CDP calls, owns one per-target snapshot registry/resolver, and returns core results. This keeps target continuity, reconnect state, protocol details, and generation invalidation in one place while giving later control features one extension seam.

### Option B — one port and handler per inspection capability

Add separate `snapshot`, `screenshot`, `inspect`, and `evaluate` methods and adapter services. This is initially simple, but the variants would be repeated in core ports, batch routing, MCP schemas, display, and validation as the operation surface grows. It also makes it easier for screenshot and action code to grow separate target/reference resolvers. Rejected because it violates the epic's single-registry and single-resolver decisions.

### Option C — expose a generic CDP command/evaluate surface

Let callers send method names and JSON through the session and build snapshot/screenshot behavior outside the adapter. This minimizes adapter code but leaks protocol/library concerns into core and MCP, weakens stable errors and provenance, and gives no trustworthy generation/actionability contract. Rejected because it breaks Ports & Adapters and turns Chrome's wire protocol into Krometrail's public API.

**Choice:** Option A. It adds one durable abstraction—the browser operation registry—that later features already need, while keeping all browser-specific identity and command mechanics private.

## Trickiest unit: snapshot generation and reference resolution

The least forgiving boundary is not AX-tree decoding; it is proving that a reference still denotes the same usable backing node when a later operation consumes it. `crates/krometrail-cdp/src/control/snapshot.rs` owns this lifecycle:

```rust
struct SnapshotRegistry {
    targets: HashMap<TargetId, TargetSnapshotRegistry>,
}

struct TargetSnapshotRegistry {
    next_generation: u64,
    active: Option<ActiveSnapshot>,
}

struct ActiveSnapshot {
    generation: SnapshotGeneration,
    attachment_generation: u64,
    document: DocumentFingerprint,
    bindings: HashMap<SnapshotNodeId, NodeBinding>,
}

struct DocumentFingerprint {
    frame_id: String,
    loader_id: String,
}

struct NodeBinding {
    backend_node_id: i64,
    actionable_at_snapshot: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceRequirement {
    Actionable,
    VisibleGeometry,
}

struct ResolvedNode {
    backend_node_id: i64,
    document_quad: [f64; 8],
}

impl SnapshotRegistry {
    fn begin_snapshot(
        &mut self,
        target_id: TargetId,
        attachment_generation: u64,
        document: DocumentFingerprint,
    ) -> Result<SnapshotGeneration, ControlError>;

    async fn resolve(
        &self,
        transport: &dyn CdpTransport,
        scope: &CommandScope,
        target_id: TargetId,
        attachment_generation: u64,
        reference: NodeReference,
        requirement: ReferenceRequirement,
    ) -> Result<ResolvedNode, ControlError>;

    fn invalidate_target(&mut self, target_id: TargetId);
    fn retain_targets(&mut self, live: impl Iterator<Item = TargetId>);
}
```

A snapshot call obtains `Page.getFrameTree`, then `Accessibility.getFullAXTree`. The decoder traverses AX `childIds` in protocol order, removes ignored and semantically empty presentation nodes while retaining descendants under their nearest retained parent, assigns non-zero local ids, and stores only private `backendDOMNodeId` bindings. It does not expose AX node ids, backend DOM ids, cdpkit values, or raw protocol properties.

Resolution applies these checks in order:

1. Request target and reference target must match.
2. The referenced generation must be the target's one active generation.
3. Current target attachment generation must equal the snapshot's generation owner.
4. A fresh `Page.getFrameTree` main-frame id/loader id must match the stored document fingerprint.
5. `DOM.describeNode`/`DOM.resolveNode` must still find the same backing node.
6. A bounded `Runtime.callFunctionOn` check must report `isConnected`, not hidden/inert/disabled/`aria-disabled`, and visible computed style; `DOM.getBoxModel` must provide finite, non-zero geometry.
7. The caller's requirement is applied. Later interaction work may add action-specific hit-testing/editability, but it must reuse this resolver rather than bypass it.

Generation mismatch or missing/replaced backing nodes map to `stale_reference` with `retry: after_recovery` and recovery `request a new structured snapshot and retry with its reference`. A currently connected but disabled/hidden/inert/geometry-less node maps to `reference_not_actionable` with recovery `refresh the snapshot or choose another target`. Selector lookup never enters this registry and is marked `selector` in screenshot provenance.

## Implementation units

### Unit 1: Core operation registry and observation contracts

**Files:**

- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/browser/operation.rs` (new)
- `crates/krometrail-core/src/browser/observation.rs` (new)
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`
- `Cargo.toml`
- `Cargo.lock`

**Story:** `epic-agent-browser-operation-page-observation-core-contracts`

The operation declaration is the only growing list:

```rust
define_browser_operations! {
    InspectPage(InspectPageRequest) => PageState {
        stable_name: "inspect_page",
        mutability: ReadOnly,
        evidence: RequestedOnly,
    },
    SnapshotPage(SnapshotPageRequest) => PageSnapshot {
        stable_name: "snapshot_page",
        mutability: ReadOnly,
        evidence: RequestedOnly,
    },
    TakeScreenshot(ScreenshotRequest) => EncodedScreenshot {
        stable_name: "take_screenshot",
        mutability: ReadOnly,
        evidence: RequestedOnly,
    },
    EvaluatePage(ReadOnlyEvaluationRequest) => EvaluationResult {
        stable_name: "evaluate_page",
        mutability: ReadOnly,
        evidence: RequestedOnly,
    },
    ObserveLive(LiveObservationRequest) => LiveObservation {
        stable_name: "observe_live",
        mutability: ReadOnly,
        evidence: LiveObservation,
    },
}

pub static BROWSER_OPERATION_REGISTRY: &[BrowserOperationDefinition];

pub trait BrowserSessionPort: Send + Sync {
    // Existing lifecycle methods remain unchanged.
    fn execute(
        &self,
        request: BrowserOperationRequest,
    ) -> PortFuture<'_, Result<BrowserOperationResult>>;
}
```

The macro generates `BrowserOperationKind`, tagged `BrowserOperationRequest`, `BrowserOperationResult`, `kind()`, `stable_name()`, and exhaustive registry tests. Result generation remains internal Rust typing; screenshot bytes are not serialized as a public JSON byte array.

Core observation contracts use validated constructors and Serde for externally meaningful metadata:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotGeneration(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotNodeId(NonZeroU32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeReference {
    pub target_id: TargetId,
    pub generation: SnapshotGeneration,
    pub node_id: SnapshotNodeId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationContext {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub started_at: SessionTime,
    pub completed_at: SessionTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CssPoint { pub x: f64, pub y: f64 }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CssSize { pub width: f64, pub height: f64 }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CssRect { pub origin: CssPoint, pub size: CssSize }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportState {
    pub layout_viewport: CssRect,
    pub visual_viewport: CssRect,
    pub content_size: CssSize,
    pub device_scale_factor: DeviceScaleFactor,
    pub page_scale_factor: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentReadiness { Loading, Interactive, Complete }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationState {
    pub current_entry_index: u32,
    pub entry_count: NonZeroU32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub readiness: DocumentReadiness,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageState {
    pub context: ObservationContext,
    pub url: String,
    pub title: String,
    pub viewport: ViewportState,
    pub navigation: NavigationState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub id: SnapshotNodeId,
    pub parent: Option<SnapshotNodeId>,
    pub depth: u16,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub properties: Vec<AccessibleProperty>,
    pub actionable: bool,
    pub reference: Option<NodeReference>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccessibleProperty {
    pub name: String,
    pub value: AccessibleValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AccessibleValue { Boolean(bool), Number(f64), Text(String) }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSnapshot {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub nodes: Vec<SnapshotNode>,
    pub omitted_node_count: u32,
}
```

Constructors enforce non-zero generations/node ids, finite CSS values, positive sizes/scales, `started_at <= completed_at`, parent-before-child preorder, unique node ids, valid depths, and `actionable == reference.is_some()` with matching target/generation. Empty page titles/names remain valid; empty roles/property names and non-finite numbers do not.

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ElementLocator {
    Reference(NodeReference),
    CssSelector(NonEmptyText),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace { ViewportCss, DocumentCss }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScreenshotTarget {
    Viewport,
    FullPage,
    Element(ElementLocator),
    Region { rect: CssRect, space: CoordinateSpace },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotRequest {
    pub target_id: TargetId,
    pub target: ScreenshotTarget,
    pub format: ImageFormat,
    pub jpeg_quality: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotMetadata {
    pub context: ObservationContext,
    pub requested_target: ScreenshotTarget,
    pub resolved_document_rect: CssRect,
    pub image: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodedScreenshot {
    metadata: ScreenshotMetadata,
    bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadOnlyEvaluationRequest {
    pub target_id: TargetId,
    pub expression: NonEmptyText,
    pub await_promise: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum EvaluationValue { Undefined, Json(serde_json::Value) }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub context: ObservationContext,
    pub value: EvaluationValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObservationPart<T> {
    Available(T),
    Unavailable(KrometrailError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveObservation {
    pub context: ObservationContext,
    pub page: ObservationPart<PageState>,
    pub snapshot: ObservationPart<PageSnapshot>,
    pub screenshot: ObservationPart<EncodedScreenshot>,
}
```

`ScreenshotRequest::new` accepts JPEG quality only for JPEG and bounds it to `0..=100`; PNG with a quality value is invalid. `EncodedScreenshot::new` rejects empty bytes and verifies dimensions from adapter metadata. `LiveObservationRequest` and `SnapshotPageRequest` contain only `target_id`; `InspectPageRequest` contains only `target_id`.

Add stable `ErrorCode` variants `StaleReference`, `ReferenceNotActionable`, `PageObservationFailed`, `ScreenshotFailed`, and `EvaluationFailed`. Central error defaults provide actionable recovery without protocol/source text. All target-scoped errors populate `ErrorContext.target_id`; stale/actionability errors use `RetryAdvice::AfterRecovery`, while invalid expressions/regions use `Never`.

**Acceptance criteria:**

- [ ] One declaration generates all five initial operation variants, result associations, stable names, mutability/evidence metadata, and exhaustive contract tests.
- [ ] Core remains runtime/transport independent; only `serde_json` is added for an explicit JSON evaluation value.
- [ ] Geometry, screenshot options, snapshot topology/reference invariants, timing, payload, and error recovery validate at constructors and Serde boundaries.
- [ ] Existing fake `BrowserSessionPort` implementations compile only after deliberately implementing `execute`; no default unavailable method hides missing adapter work.

### Unit 2: Single-writer operation dispatch and current-state/evaluation adapter

**Files:**

- `crates/krometrail-cdp/src/control/mod.rs` (new)
- `crates/krometrail-cdp/src/control/evaluation.rs` (new)
- `crates/krometrail-cdp/src/control/tests.rs` (new)
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs`
- existing test fakes implementing `BrowserSessionPort`

**Story:** `epic-agent-browser-operation-page-observation-operation-executor`

```rust
pub(crate) struct PageControl {
    clock: Arc<dyn MonotonicClock>,
    session_id: SessionId,
    session_origin: SessionOrigin,
    snapshots: SnapshotRegistry,
    config: PageControlConfig,
}

impl PageControl {
    pub(crate) async fn execute(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: BrowserOperationRequest,
    ) -> Result<BrowserOperationResult>;
}

enum SupervisorCommand {
    Input(SupervisorInput),
    Execute(
        BrowserOperationRequest,
        oneshot::Sender<Result<BrowserOperationResult>>,
    ),
    Stop(oneshot::Sender<Result<BrowserStopOutcome>>),
}
```

`ProductionSession::execute` enqueues one command and awaits its oneshot result. The supervisor validates `Ready`, resolves a live `SupervisorTargetState` by Krometrail `TargetId`, requires `Attached` plus a current `transport_session`, and dispatches against the current connection. During reconnect it immediately returns `browser_disconnected` with after-recovery advice; during stop/end it returns the corresponding stable failure. Every reconnect wait/interrupt branch explicitly answers queued `Execute` commands rather than dropping their senders or replaying an operation against a rebuilt target.

`PageControl` receives the root-injected `MonotonicClock` and existing session origin even when capture is disabled; production composition passes the same clock used by recording. It samples operation start/completion around commands and normalizes to `SessionTime`.

`inspect_page` uses three bounded calls: one `Runtime.evaluate` expression returning `location.href`, `document.title`, `document.readyState`, and `devicePixelRatio`; `Page.getLayoutMetrics`; and `Page.getNavigationHistory`. It validates every required field, derives back/forward flags from history index/count, and rejects non-finite/invalid geometry rather than substituting zeroes.

`evaluate_page` sends:

```rust
serde_json::json!({
    "expression": request.expression.as_str(),
    "returnByValue": true,
    "awaitPromise": request.await_promise,
    "throwOnSideEffect": true,
    "silent": true,
    "timeout": config.evaluation_timeout.as_millis() as u64,
})
```

It distinguishes undefined from JSON null, rejects `exceptionDetails`, remote `objectId`-only values, unserializable values, and results whose serialized JSON exceeds `MAX_EVALUATION_RESULT_BYTES` (1 MiB). Page exception text is reduced to a bounded caller-useful message; raw stack traces, object previews, URLs, and source transport errors remain debug-only.

**Acceptance criteria:**

- [ ] Operations execute only against the exact current target flat session and report session/target/attachment/timing provenance.
- [ ] Reconnect, stop, closure, missing target, unavailable session, command timeout, unsupported command, malformed response, and queue closure all resolve callers with stable actionable errors; no operation oneshot hangs or auto-replays.
- [ ] Inspection reports fresh URL/title/readiness/layout/visual/content/device-scale/history state, not only the possibly stale target projection.
- [ ] Evaluation is demonstrably read-only, bounded, by-value, and explicit about undefined, exception, side-effect refusal, unsupported values, and timeout.

### Unit 3: Compact AX snapshot and shared reference resolver

**Files:**

- `crates/krometrail-cdp/src/control/snapshot.rs` (new)
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/session.rs`

**Story:** `epic-agent-browser-operation-page-observation-snapshot-references`

Implement the tricky unit exactly as specified above. Keep bounds adapter-owned and fixed initially:

```rust
const MAX_SNAPSHOT_NODES: usize = 5_000;
const MAX_SNAPSHOT_TEXT_BYTES: usize = 1 << 20;
const MAX_ACCESSIBLE_PROPERTY_COUNT: usize = 32;
```

The snapshot decoder retains only the accessibility property allowlist needed for compact agent use (`disabled`, `editable`, `expanded`, `focused`, `focusable`, `haspopup`, `invalid`, `level`, `multiline`, `multiselectable`, `orientation`, `pressed`, `readonly`, `required`, `selected`, `checked`). One local declaration drives allowlisting and output names. Unknown/additive AX properties are ignored, not fatal. Roles remain strings so a new Chrome role cannot terminate the observation path.

Actionable-at-snapshot is true only when the node has `backendDOMNodeId`, is not ignored/disabled/hidden, and either has an interaction-oriented role or an AX focusable/editable/clickable signal. The actionability role/signal declaration lives beside the decoder and is tested once. This is a candidate hint, never proof for dispatch; every consumer must call the resolver.

Generation allocation uses checked monotonic `u64` increments per Krometrail target. Exhaustion returns `page_observation_failed`; it never wraps and revives an old reference. The active generation is installed only after complete response decode and invariant validation, so a failed refresh does not corrupt the previous usable snapshot. A successful newer snapshot atomically replaces it.

Selectors use a separate one-shot helper (`DOM.getDocument` + `DOM.querySelector`) and then the same backing-node/actionability/geometry checks. They do not receive or enter a snapshot generation. Selector not found returns `not_found`; invalid selector returns `invalid_input`; both carry target context and selector-refresh guidance without echoing arbitrary page content.

**Acceptance criteria:**

- [ ] AX output is deterministic, compact, bounded, tolerant of additive fields, and preserves parent-before-child preorder with an exact omitted count.
- [ ] Only actionable candidates get references; every returned reference binds the selected target and active generation.
- [ ] A newer snapshot, navigation/loader change, reconnect/attachment change, target closure, missing backing node, or generation mismatch cannot resolve an old reference.
- [ ] Connected-but-disabled/hidden/inert/non-visible nodes fail `reference_not_actionable`; no resolver guesses a replacement by role/name/selector.
- [ ] Shadow-DOM and iframe AX nodes either resolve through verified backend nodes or are omitted/failed explicitly; no cross-target or cross-document reference is fabricated.

### Unit 4: Screenshot capture and honest live observation

**Files:**

- `crates/krometrail-cdp/src/control/screenshot.rs` (new)
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/capture/image_header.rs`
- `crates/krometrail-cdp/src/capture/mod.rs`

**Story:** `epic-agent-browser-operation-page-observation-screenshots-live-observation`

Make the existing bounded PNG/JPEG header reader `pub(crate)` and reuse it; do not add a second image decoder or the `image` crate. `Page.captureScreenshot` mapping is:

- viewport — no clip, `captureBeyondViewport: false`;
- full page — fresh `Page.getLayoutMetrics.cssContentSize` document clip, `captureBeyondViewport: true`;
- reference element — resolve through `SnapshotRegistry`, take the finite bounding rectangle of the returned document quad, `captureBeyondViewport: true`;
- selector element — one-shot selector resolution followed by the same actionability/geometry path and provenance marked selector;
- document region — exact requested document CSS clip;
- viewport region — add current `cssLayoutViewport.pageX/pageY`, verify the entire region lies in the current viewport, then capture the exact document clip.

All explicit clips use `scale: 1.0` in CSS-space CDP terms. Base64 decoding is bounded before allocation, empty/malformed image data fails, header dimensions are authoritative, and request/result format must agree. Requests outside their declared viewport/content extent fail `invalid_input`; they are not silently clamped. Chrome command failure maps to `screenshot_failed` or `unsupported` with detected browser/protocol details available through session compatibility but no raw endpoint/source error.

`observe_live` captures one initial target/attachment binding and operation window, then invokes the same internal inspection, snapshot, and viewport-screenshot functions in that order. Each component retains its own narrower context. If the transport or attachment is lost, remaining parts become `Unavailable` rather than being attempted against another generation. It never starts/stops screencast capture and never reads the continuous recorder as a substitute for a current screenshot.

**Acceptance criteria:**

- [ ] Viewport, full-page, reference element, selector element, and declared viewport/document region requests map to the exact intended clip and return validated payload/geometry/scale/target provenance.
- [ ] Reference screenshots use the one shared resolver; selector screenshots identify weaker provenance and cannot create durable references.
- [ ] High-DPI/device-scale metadata is measured, not inferred from encoded dimensions or host platform.
- [ ] Live observation reports each available or failed part explicitly, remains bound to one target attachment, and is reusable by later state-changing operations without MCP or timeline persistence.
- [ ] Screenshot work is independent of continuous capture and reuses one bounded header parser.

### Unit 5: Deterministic and real-browser qualification

**Files:**

- `crates/krometrail-cdp/tests/page_observation.rs` (new)
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs`
- `crates/krometrail-cdp/tests/support/mod.rs`
- `tests/fixtures/browser/page-observation/index.html` (new)
- `tests/fixtures/browser/README.md`

**Story:** `epic-agent-browser-operation-page-observation-qualification`

Extend the existing scripted raw transport rather than introducing a second fake protocol stack. Deterministic tests assert exact session scope and command JSON, AX additive-field tolerance, generation replacement, loader/attachment invalidation, malformed response handling, operation completion during reconnect/stop, CSS-to-document region conversion, header/format rejection, evaluation limits, and partial live-observation behavior without sleeps.

Add one dependency-free observation fixture containing named controls, dynamic node replacement, disabled/hidden/inert states, a scrollable full page, a shadow root, a same-origin iframe, and known CSS rectangles. Opt-in real Chrome tests use the production connector and operation port to verify:

1. fresh URL/title/viewport/navigation state and side-effect-free evaluation;
2. compact snapshot role/name/property output and actionable references;
3. stale generation after refresh and stale backing node after deterministic replacement;
4. viewport/full-page/reference/selector/viewport-region/document-region screenshots with decoded dimensions and exact provenance;
5. shadow-DOM and iframe behavior without cross-document identity guesses;
6. default- and forced-high-DPI observations where the host honors the flag, recording actual scale rather than asserting a fabricated value;
7. one valid live observation and one injected partial screenshot failure that retains honest page/snapshot evidence.

The real-browser test is opt-in under `KROMETRAIL_REAL_CHROME_TESTS=1`; deterministic contract coverage remains in the default suite. Hosted macOS/high-DPI evidence is useful qualification but is not required to invent an unavailable display mode: report an unsupported/ignored forced scale honestly and preserve it for the epic evaluation lane.

**Acceptance criteria:**

- [ ] Default deterministic tests protect the stable operation/reference/error/provenance seams without depending on Chrome timing.
- [ ] Production-connector Chrome tests cover dynamic replacement, scrolling, shadow DOM, iframe, and screenshot variants on Linux; platform/scale observations remain explicit.
- [ ] The fixture is target-only, dependency-free, documented, and introduces no second Krometrail runtime.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass.

## Error and actionability semantics

| Condition | Stable code | Retry | Recovery |
| --- | --- | --- | --- |
| Unknown/closed target | `not_found` or `target_failed` | safe only when target still exists | refresh/list targets and choose an attached page |
| Session reconnecting | `browser_disconnected` | after recovery | wait for ready status, then repeat the read-only operation |
| Reference target/generation/document/backing node expired | `stale_reference` | after recovery | request a new structured snapshot and retry with its reference |
| Backing node exists but is hidden, inert, disabled, detached, or geometry-less | `reference_not_actionable` | after recovery | refresh the snapshot or choose another target |
| Selector absent | `not_found` | safe | refresh inspection or use a current snapshot reference |
| Invalid selector/region/quality/expression boundary | `invalid_input` | never without correction | correct the request |
| Unsupported Page/DOM/Accessibility/Runtime behavior | `unsupported` | after recovery | use a compatible renderer/version or another supported target form |
| Malformed AX/layout/history/screenshot response | `page_observation_failed` / `screenshot_failed` | safe | retry once; if repeated, inspect compatibility/browser status |
| Evaluation exception, side-effect refusal, timeout, remote-only or oversized value | `evaluation_failed` | never without correction | use a bounded side-effect-free expression returning a JSON value |

Raw CDP response text, endpoint, browser target key, transport session id, object id, backend node id, AX node id, page stack trace, and screenshot bytes never enter info logs or stable error messages.

## Implementation order

1. `epic-agent-browser-operation-page-observation-core-contracts`
2. `epic-agent-browser-operation-page-observation-operation-executor`
3. `epic-agent-browser-operation-page-observation-snapshot-references`
4. `epic-agent-browser-operation-page-observation-screenshots-live-observation`
5. `epic-agent-browser-operation-page-observation-qualification`

One feature owner should normally carry all five checkpoints. The dependencies expose the contract and verification order; they are not five parallel worker assignments.

## Simplification

- Replace the deferred snapshot/reference placeholders with one core contract, one per-target active-generation registry, and one adapter resolver.
- Extend the existing single-writer production supervisor and exact transport-session binding; do not add a browser-control session map, reconnect loop, active-page registry, or Electron-specific path.
- Generate operation request/result associations and metadata from one declaration. Later features extend it instead of hand-copying standalone/batch/MCP variant sets.
- Reuse `CdpTransport::send_raw`, `ImageFormat`, `PixelDimensions`, `DeviceScaleFactor`, `ObservationContext` timing primitives, stable core errors, and the bounded capture image-header reader.
- Keep selector and coordinate targeting as explicit weaker request forms, not identities or compatibility aliases.
- No test is proposed for trivial getters or every AX property. No old contract needs a compatibility shim because the snapshot/reference types were intentionally deferred.

## Testing

- **Core interface tests:** Exhaustively prove the macro registry and request/result association, and protect validation/Serde for references, geometry, snapshot topology, screenshot metadata, evaluation values, partial observation, and stable errors. These are public contract risks.
- **Scripted adapter tests:** Protect exact flat-session routing, actor completion, generation/document invalidation, malformed/additive protocol handling, screenshot clip conversion, and bounded read-only evaluation. These are deterministic failure and concurrency risks.
- **Opt-in real Chrome test:** Protect the assumptions scripted JSON cannot establish: actual AX-to-DOM backing ids, shadow/iframe behavior, screenshot geometry/device scale, and live observation through the production supervisor.
- **Test removal/consolidation:** Move the existing image-header tests with the parser only if visibility changes; do not duplicate parser vectors under control tests. Extend the existing `ScriptedCdp` and Chrome wrapper helpers rather than adding observation-only fakes for the same seams.

## Risks

- **AX/DOM identity across frames:** OOPIF iframe nodes may not resolve in the parent flat session even when they appear in accessibility output. The decoder must omit or explicitly fail such references rather than claim cross-session identity. The real-browser checkpoint determines the supported envelope; future cross-frame control can extend the resolver with explicit frame ownership.
- **Actionability is action-specific:** A general visible/enabled check cannot prove pointer hit-testing, editability, or overlay clearance for every future action. This feature establishes a strict common floor; the interaction feature adds action-specific checks after calling the same resolver.
- **Snapshot size and fidelity:** Fixed compaction limits may omit useful deep content. Truncation is explicit with an omitted count, and later filter/pagination work can extend the request without weakening current reference semantics.
- **Page mutation between component calls:** Inspection, snapshot, and screenshot are not a browser transaction. Component and aggregate timing plus target/attachment provenance make that honest; the design does not freeze the page or claim pixel/AX simultaneity.
- **Read-only evaluation compatibility:** `throwOnSideEffect` can reject expressions that appear observational but invoke getters or unsupported runtime paths. Explicit refusal is safer than hidden mutation; callers can use direct snapshot/inspection or simplify the expression.
- **Screenshot memory:** Full-page screenshots can be large. The adapter must bound base64 length before decoding and fail oversized output; durable/resource streaming belongs to the MCP/storage features rather than an unbounded core payload.
- **High-DPI hosted behavior:** Headless Chrome may ignore forced scale flags, as prior capture qualification observed. Tests report measured scale and dimensions; they never relabel scale `1.0` as high DPI. Platform evidence can be deferred without weakening deterministic contracts.

## Pre-mortem

The riskiest assumption is that one renderer session can map useful AX nodes to live backend DOM nodes across ordinary shadow/iframe structures. Failure would show up as snapshots that look correct but references that consistently fail or, worse, resolve in the wrong document. The design prevents the dangerous form by binding target, generation, attachment, main-frame loader, and backend node and by refusing replacement guesses. If real-browser qualification shows OOPIF resolution is not sound, the fallback is to omit those actionable references while retaining screenshot/coordinate evidence and record the supported boundary; cross-frame session ownership becomes explicit later work. The area of least certainty is Chrome's side-effect-analysis behavior for awaited expressions, so evaluation remains intentionally narrow and has direct snapshot/inspection fallbacks.

## Implementation summary

One feature owner completed the five dependency-ordered checkpoints without splitting the shared control/reference context:

- `epic-agent-browser-operation-page-observation-core-contracts` — added validated infrastructure-free page/snapshot/reference/screenshot/evaluation/live-observation values, the macro-backed five-operation registry, required `BrowserSessionPort::execute`, and stable observation error semantics.
- `epic-agent-browser-operation-page-observation-operation-executor` — routed operations through the existing single-writer production supervisor, exact current flat-session binding, one injected monotonic clock/session origin, fresh inspection, and bounded side-effect-free evaluation. Reconnect paths answer without replay.
- `epic-agent-browser-operation-page-observation-snapshot-references` — added deterministic bounded AX compaction, checked per-target generations, atomic refresh, document/attachment/backing-node invalidation, and one shared live resolver plus weaker selector lookup.
- `epic-agent-browser-operation-page-observation-screenshots-live-observation` — added every declared screenshot target with exact CSS clip conversion, bounded base64/header validation, measured device scale, and reusable honest partial live observation independent of continuous capture.
- `epic-agent-browser-operation-page-observation-qualification` — extended the shared scripted transport, added a standalone fixture and deterministic production-port coverage, and proved the supported AX/DOM/screenshot/live path against real Linux Chrome including forced-scale measurement.

The implementation stayed inside core/CDP and target-fixture boundaries. It did not register MCP tools, add navigation/input/wait/batch operations, touch storage or temporal vision, or create another browser/session manager. Infrastructure and CDP identities remain private to the adapter.

### Implementation discoveries and deviations

- Generated operation result payloads are uniformly boxed. Clippy exposed a large inline enum layout once `LiveObservation` joined the registry; boxing preserves the single generated association surface while avoiding every dispatch value reserving the largest payload.
- Snapshot target pruning occurs at operation dispatch, while attachment generation and document fingerprint are synchronously rechecked during resolution. This gives the required stale behavior without another target-event subscriber or control-state writer.
- Real Chrome occasionally returned one retry-safe screenshot failure while rapidly switching surface dimensions across viewport/full-page/element/region variants. Qualification allows exactly one retry only for `screenshot_failed` and still requires a valid payload/provenance; persistent failure remains red.
- Linux forced-scale qualification measured device scale `2`. macOS/high-DPI hosted evidence was unavailable in this endpoint and remains explicitly unclaimed rather than inferred.

## Integrated verification

- `cargo fmt --all -- --check` passed.
- `cargo check --workspace --all-targets --locked` passed.
- `cargo test --workspace --all-targets --locked` passed: 221 tests across 22 suites.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` passed.
- `cargo test -p krometrail-cdp --all-targets --locked` passed: 153 tests across 15 suites.
- `cargo test -p krometrail-cdp --test page_observation --locked -- --nocapture` passed: 8 deterministic/default tests.
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test page_observation --locked -- --nocapture` passed: all 8 tests, including default and forced-scale real Chrome qualification on Linux.

All five child stories are `done`; integrated implementation is complete and this feature is ready for the caller's standard independent review. It is not self-approved here.

## Review findings (2026-07-13)

Standard cross-model review found one receiver-confirmed current-cycle blocker:

- **Honor screenshot-only `VisibleGeometry` resolution.** `ReferenceRequirement` is passed through
  the shared resolver but ignored, so element and selector screenshots reject disabled yet visible
  controls with `reference_not_actionable`. Screenshot capture needs connection, visibility, and
  finite geometry, not interaction enablement. Make `VisibleGeometry` skip disabled/inert action
  gating while keeping hidden/disconnected/geometry-less refusal; retain the strict `Actionable`
  path for durable action references. Add focused scripted and/or real-fixture regression coverage.

The reviewer also proposed an O(n²) bounded snapshot-depth lookup, conservative selector retry
classification, and constructor-description drift. These are non-blocking nits at the current
5,000-node bound and explicit-failure posture. A workspace failure observed during review belonged
to concurrent segment-format work and is not a page-observation finding.

## Review remediation (2026-07-13)

The receiver-confirmed blocker is resolved:

- Split the live DOM check into `connected`, `visuallyHidden`, and `interactionBlocked` facts. HTML
  `inert` and disabled/`aria-disabled` state affect focus and interaction, not whether pixels are
  painted; real Chrome confirmed that visible disabled and inert controls retain screenshotable
  geometry. `visuallyHidden` is limited to actual rendering suppression (`hidden`, `display:none`,
  hidden/collapsed visibility, and hidden content visibility).
- Both reference and selector element screenshots now request `VisibleGeometry`. A reference minted
  while actionable can therefore still capture the same connected visible node after it becomes
  disabled or inert. The resolver continues to reject stale/disconnected identities, visually
  hidden nodes, malformed state, and missing/non-finite/zero-area geometry for both requirements.
- `Actionable` remains strict: native disabled state, `aria-disabled`, and inherited light-DOM
  `inert` return `reference_not_actionable`. The adapter-owned parent walk detects inert ancestors
  without asking Chrome's side-effect analyzer to approve a selector query.
- Focused unit coverage applies the same blocked-but-visible state to both requirements and proves
  only `Actionable` fails; both still reject hidden and disconnected nodes. Scripted production-port
  coverage proves disabled visible reference and selector screenshots return validated images, and
  the real fixture proves actual Chrome screenshot capture for `#disabled-action` and an inert
  descendant.

### Remediation verification

- `rustfmt --edition 2024 --check` on the owned control and qualification files passed.
- `cargo fmt --all -- --check` passed without editing concurrent files.
- `cargo test -p krometrail-cdp --lib --locked` passed: 74 tests.
- `cargo test -p krometrail-cdp --test page_observation --locked` passed: 8 tests.
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test page_observation opt_in_real_chrome_observes_fixture_and_all_screenshot_target_families --locked -- --nocapture` passed against real Linux Chrome, including disabled and inert selector screenshots.
- `cargo check -p krometrail-cdp --all-targets --locked` passed.
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` passed.
- `cargo check --workspace --all-targets --locked` and `cargo test --workspace --all-targets --locked` passed; the latter ran 246 tests across 24 suites against the concurrent segment-format tree.

A transient `--locked` mismatch occurred while the separate segment-format owner changed root Cargo
metadata ahead of its lockfile update. The same focused command passed once that external update
settled. No segment-format, temporal-vision, composition-root, Cargo, or work-view file was edited
or staged by this remediation. The feature is returned to `review` for independent approval; it is
not self-approved.
