---
id: epic-agent-browser-operation-verified-interactions
kind: feature
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-page-observation]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Verified Browser Interactions

## Brief

Let agents act on the observed page through reference-first click, fill/type, key input, selection, hover, drag, scroll, file upload, and dialog operations, with declared coordinate-space fallback for DOM-opaque content. Each operation creates an interaction record before dispatch, validates its target at the last responsible moment, applies action-appropriate completion, and returns an explicit post-action live observation and timeline anchor.

Use one action registry to drive variants, validation, routing, sanitized interaction parameters, and stable display instead of bespoke public contracts per action. This feature owns standalone input execution and explicit stale/no-op failures; page lifecycle, caller-requested waits, ordered batching, durable interaction persistence, and MCP handlers remain with their owning features or epics.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: sibling consumer of `epic-agent-browser-operation-page-observation`; combines with page lifecycle before waits and batches are integrated
- Inherited decisions: snapshot references are primary, selectors and coordinates are weaker declared fallbacks, and silent or guessed success is forbidden

## Simplification opportunity

- Route all interaction variants through one registry and executor shape with narrowly action-specific CDP mechanics and completion policies. Avoid one service, schema, error taxonomy, and result envelope per input command.

## Foundation references

- `docs/VISION.md` — Product Thesis and Core Experience
- `docs/SPEC.md` — Current-State Observation, Structured Page Snapshots, Browser-Control Surface, and Action Timeline
- `docs/ARCHITECTURE.md` — Structured Snapshots and References and Interaction Execution
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Design decisions

- **Dispatch:** Direct local probes only. The caller prohibited subagents and peer review, and the completed page-observation feature (snapshot resolver, screenshot clip conversion, single-writer supervisor, `observe_live`), the rust-cdp-transport skill (`Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`, `Input.insertText`, `DOM.setFileInputFiles`, `Page.handleJavaScriptDialog`, `DOM.scrollIntoViewIfNeeded`), the existing fixture/test patterns, and the empty reserved MCP crate resolve the design without another discovery path.
- **One operation registry, extended with action metadata:** The existing `define_browser_operations!` macro and `BROWSER_OPERATION_REGISTRY` slice already drive observation operations. Interactions extend the same macro and slice, so standalone and (later) batch + MCP variants all derive from one declaration. Each entry gains an optional `ActionDefinition` carrying category, actionability floor, accepted locator shape, completion kind, and a stable display name; observation entries carry `None`. No parallel enum, schema, or dispatch table is created.
- **Locator as evidence, not identity:** `InteractionLocator` is `Element(ElementLocator) | Coordinate { point, space }`. Element references and selectors reuse the page-observation resolver; coordinates declare their space (`ViewportCss`/`DocumentCss`) and convert through the same fresh layout viewport used by screenshot regions. Coordinates never mint references and never enter the snapshot generation.
- **Actionability is action-specific and lives in the shared resolver:** The page-observation `ReferenceRequirement` enum gains `Editable`, `Selectable`, and `FileInput` variants alongside the existing `Actionable`/`VisibleGeometry`. One `Runtime.callFunctionOn` returns `{connected, visuallyHidden, interactionBlocked, tagName, inputType, isEditable, isSelect, isFileInput}`; `validate_node_state` consumes the action-specific subset. Per-action verification does not bypass the resolver and does not add unbounded extra round-trips.
- **Coordinate no-op is explicit:** Element-targeted actions reuse the resolver's stale/disabled/hidden refusal. Coordinate actions perform one `Document.elementFromPoint` hit-test after layout conversion; an empty/disabled result returns `InteractionFailed` (`no_hit_target`) rather than a silent dispatch into empty space. Krometrail never claims success for input it cannot aim.
- **Interaction record before dispatch:** The adapter allocates an `InteractionId` from the session-wide `IdSource` and builds a partial `InteractionRecord` (id, context-start, action kind, sanitized params, locator summary) before any CDP input command. Dispatch, completion, and live-observation timestamps fill in as the operation progresses. Standalone records always carry `parent_batch: None`; batch (later) populates the parent. The record is returned in the result; durable persistence stays owned by `epic-durable-browser-memory` through a port this feature does not wire.
- **Completion is owned by the adapter and derived from action kind:** Each `ActionDefinition` declares a `CompletionKind` (`InputAcknowledged` for scroll/dialog/upload, `Settled` for click/hover/fill/select/drag/press). `Click` and `PressKeys` requests accept an optional `wait_for_navigation: bool` that escalates `Settled` to bounded `NavigationAware` (subscribe to `Page.lifecycleEvent` for one bounded window). No global network-idle policy is implied; the continuous recorder stays independent.
- **Post-action evidence reuses `observe_live`:** After completion, every standalone action runs the same internal `inspect → snapshot → viewport-screenshot` sequence used by `observe_live`, returning a full `LiveObservation` alongside the record. The freshly installed snapshot generation becomes the new active generation, exactly as a manual `observe_live` would. Honest partial-failure semantics carry over unchanged.
- **Sanitization is per-request, driven by the registry kind:** Each interaction request implements `sanitize(&self) -> SanitizedParameters`. The constructor bounds serialized size (4 KiB), requires a JSON object, and reduces sensitive payloads to non-sensitive metadata (`Fill` value length, dialog prompt-text length, upload file count + basenames, select value length). The registry's `ActionDefinition` carries the stable display name and category; sanitization never echoes raw CDP method names, object ids, backend node ids, transport session ids, or fill values.
- **File/path security is local-first but explicit:** `ValidatedFilePath` is constructed in core as an absolute, normalized, UTF-8 path with no `..` components after normalization, bounded component count (32), bounded byte length (4 KiB), and a bounded file count per request (8). The adapter canonicalizes through `std::fs::canonicalize` at dispatch, verifies existence and read permission, and rejects symlinks escaping the canonical root before invoking `DOM.setFileInputFiles`. Local-agent authority is the local user; there is no remote/sandbox boundary, but no unbounded or relative path reaches Chrome.
- **Key chord grammar is closed and validated:** `KeyChord` wraps `NonEmptyText` parsed at construction into `Modifier` (`Alt`/`Control`/`Shift`/`Meta`) plus `NamedKey` (CDP-recognized key names) plus a single Unicode `char`. Unknown multi-char tokens reject with `InvalidInput`. The adapter translates the parsed segments into the right `Input.dispatchKeyEvent` (rawKeyDown/char/keyUp) sequence; `Fill` uses `Input.insertText` for atomic value replacement.
- **One new error code:** Add `ErrorCode::InteractionFailed` for CDP input rejection, completion timeout, no-hit-target, and upload-path dispatch failure. Stale references, actionability failures, missing dialogs, missing selectors, invalid input forms, transport loss, and cancellation reuse the existing stable codes. Default retry for `InteractionFailed` is `Safe` for `InputAcknowledged` actions and `Never` for `Settled`/`NavigationAware` actions whose effect may have partially applied.
- **IdSource plumbing:** `ProductionBrowserConnector` already injects a `MonotonicClock` into `PageControl`; it now also injects an `Arc<dyn IdSource>`. When capture is enabled the connector shares the capture assembly's source; otherwise it falls back to a `UuidIdSource` (cdp-owned) that mirrors the existing `Uuid::new_v4()` session-id fallback. `PageControl::new` takes the `IdSource`; no second id allocator or interaction store is created.
- **UI surface:** This is an agent/API control surface, not a human screen or journey. No mockups apply (parent epic defers mockups for the entire control surface).

## Architectural choice

### Option A — registry-driven, single-executor interaction dispatch (chosen)

Interactions extend `define_browser_operations!` and `BROWSER_OPERATION_REGISTRY`. The adapter adds one `control/interaction.rs` module owning the pre-dispatch record, locator resolution, coordinate conversion, hit-testing, completion awaiting, and post-action `observe_live` reuse. Action-family modules (`pointer.rs`, `keyboard.rs`, `form.rs`, `upload.rs`, `dialog.rs`) translate their action-specific CDP mechanics and return through the shared executor. This keeps one operation port, one supervisor command path, one resolver, and one observation contract while letting narrowly different CDP shapes live beside their action.

### Option B — one port and handler per interaction capability

Add `click`, `fill`, `press`, `select`, etc. methods to `BrowserSessionPort` with per-action adapter services. Initially simple, but each variant is then duplicated in the port, batch routing, MCP schemas, validation, and display. It also makes it easy for input code to grow a parallel reference/coordinate resolver or per-action completion policy. Rejected because it violates the epic's single-registry and single-resolver decisions and the parent epic's explicit simplification arc.

### Option C — generic CDP input surface

Expose `Input.dispatchMouseEvent` / `Input.dispatchKeyEvent` style pass-through. Minimizes adapter code but leaks the protocol surface into core and MCP, removes action-specific actionability/completion, and makes interaction records impossible to sanitize uniformly. Rejected for the same Ports & Adapters reasons the page-observation feature rejected the generic CDP surface.

**Choice:** Option A. It grows the existing registry and executor seams with one new durable abstraction (action metadata + record) instead of nine parallel contracts, while keeping every Chrome-specific identity and command private to the adapter.

## Trickiest unit: action-specific actionability and the no-op boundary

The least forgiving boundary is not the CDP call shape; it is proving that the resolved target is *actionable for this specific action* and refusing to claim success when input cannot take effect. `crates/krometrail-cdp/src/control/snapshot.rs` already owns the live-DOM actionability check; this feature extends it without forking the resolver.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceRequirement {
    VisibleGeometry,
    Actionable,
    Editable,    // Actionable + editable text input/textarea/contenteditable
    Selectable,  // Actionable + tagName == SELECT
    FileInput,   // Actionable + tagName == INPUT && type == file
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    pub(crate) document_quad: [f64; 8],
}
```

The resolver's `Runtime.callFunctionOn` returns:

```javascript
function () {
  const s = getComputedStyle(this);
  let n = this, inert = false;
  while (n && !inert) { inert = n.inert === true; n = n.parentElement; }
  const tag = this.tagName;
  return {
    connected: this.isConnected,
    visuallyHidden: this.hidden || s.display === 'none'
      || s.visibility === 'hidden' || s.visibility === 'collapse'
      || s.contentVisibility === 'hidden',
    interactionBlocked: inert || this.disabled || this.getAttribute('aria-disabled') === 'true',
    tagName: tag,
    inputType: tag === 'INPUT' ? (this.type || 'text') : null,
    isEditable: !this.readOnly && !this.disabled &&
      (this.isContentEditable ||
       tag === 'INPUT' && /^(text|search|url|email|tel|password|number)$/.test(this.type || 'text') ||
       tag === 'TEXTAREA'),
    isSelect: tag === 'SELECT',
    isFileInput: tag === 'INPUT' && (this.type || 'text').toLowerCase() === 'file',
  };
}
```

`validate_node_state` applies a common floor (connected, not visually hidden) and then the action-specific requirement:

| Requirement | Extra conditions |
| --- | --- |
| `VisibleGeometry` | none (screenshot/hover floor) |
| `Actionable` | `!interactionBlocked` |
| `Editable` | `Actionable` + `isEditable` |
| `Selectable` | `Actionable` + `isSelect` |
| `FileInput` | `Actionable` + `isFileInput` |

Coordinate-only actions (no element) bypass the resolver entirely and instead use a `Document.elementFromPoint(x, y)` hit-test after viewport→document conversion. A null/empty result maps to `InteractionFailed` (`no_hit_target`); a disabled/inert hit element is allowed (coordinates do not require interaction enablement, matching the screenshot floor) but the dispatch proceeds honestly and the post-action observation reflects whatever happened.

Generation mismatch, attachment change, document/loader change, missing backend node, and disconnected nodes keep their existing `stale_reference` mapping; hidden/geometry-less nodes keep `reference_not_actionable`. The new action-specific failures (not editable, not a select, not a file input) also map to `reference_not_actionable` with an action-specific recovery message so the caller can refresh or pick another target.

## Implementation units

### Unit 1: Core interaction contracts and registry extension

**Files:**

- `crates/krometrail-core/src/browser/interaction.rs` (new)
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`

**Story:** `epic-agent-browser-operation-verified-interactions-core-contracts`

The operation declaration gains an optional `action: $action:expr` field. Observation entries pass `action: None`; interaction entries reference a `const ACTION_<NAME>: ActionDefinition`. The macro emits the additional `BrowserOperationKind`, `BrowserOperationRequest`, and `BrowserOperationResult` variants and extends `BrowserOperationDefinition` with `action: Option<&'static ActionDefinition>`.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Pointer,
    Keyboard,
    Form,
    Scroll,
    DragDrop,
    FileDialog,
    Dialog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionabilityRequirement {
    Actionable,
    VisibleGeometry,
    CoordinateOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedLocator {
    Element,            // reference or selector only
    ElementOrCoordinate,// pointer/hover/drag-source/scroll-target
    CoordinateOnly,     // currently unused; reserved for explicit coord-only tools
    None,               // target-wide actions (dialog)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    InputAcknowledged,
    Settled,
    NavigationAware,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionDefinition {
    pub category: ActionCategory,
    pub actionability: ActionabilityRequirement,
    pub locator: AcceptedLocator,
    pub completion: CompletionKind,
    pub display_name: &'static str,
}
```

The nine interaction variants join the registry:

```rust
define_browser_operations! {
    // observation entries unchanged in shape; action: None added
    InspectPage(InspectPageRequest) => PageState {
        stable_name: "inspect_page", mutability: ReadOnly,
        evidence: RequestedOnly, action: None,
    },
    SnapshotPage(SnapshotPageRequest) => PageSnapshot {
        stable_name: "snapshot_page", mutability: ReadOnly,
        evidence: RequestedOnly, action: None,
    },
    TakeScreenshot(ScreenshotRequest) => EncodedScreenshot {
        stable_name: "take_screenshot", mutability: ReadOnly,
        evidence: RequestedOnly, action: None,
    },
    EvaluatePage(ReadOnlyEvaluationRequest) => EvaluationResult {
        stable_name: "evaluate_page", mutability: ReadOnly,
        evidence: RequestedOnly, action: None,
    },
    ObserveLive(LiveObservationRequest) => LiveObservation {
        stable_name: "observe_live", mutability: ReadOnly,
        evidence: LiveObservation, action: None,
    },
    // interaction entries
    Click(ClickRequest) => InteractionResult {
        stable_name: "click", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_CLICK),
    },
    Fill(FillRequest) => InteractionResult {
        stable_name: "fill", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_FILL),
    },
    PressKeys(PressKeysRequest) => InteractionResult {
        stable_name: "press_keys", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_PRESS_KEYS),
    },
    SelectOption(SelectOptionRequest) => InteractionResult {
        stable_name: "select_option", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_SELECT),
    },
    Hover(HoverRequest) => InteractionResult {
        stable_name: "hover", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_HOVER),
    },
    Drag(DragRequest) => InteractionResult {
        stable_name: "drag", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_DRAG),
    },
    Scroll(ScrollRequest) => InteractionResult {
        stable_name: "scroll", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_SCROLL),
    },
    UploadFiles(UploadFilesRequest) => InteractionResult {
        stable_name: "upload_files", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_UPLOAD),
    },
    HandleDialog(HandleDialogRequest) => InteractionResult {
        stable_name: "handle_dialog", mutability: StateChanging,
        evidence: LiveObservation, action: Some(&ACTION_DIALOG),
    },
}
```

Each `ACTION_*` is a `const ActionDefinition` declared once beside the macro.

Locator, request, and result contracts in `interaction.rs`:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum InteractionLocator {
    Element(ElementLocator),
    Coordinate { point: CssPoint, space: CoordinateSpace },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton { Left, Middle, Right }

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Modifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KeyChord(NonEmptyText);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier { Alt, Control, Shift, Meta }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Enter, Tab, Escape, Backspace, Delete, Space,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum KeySegment { Modifier(Modifier), NamedKey(NamedKey), Char(char) }

impl KeyChord {
    pub fn new(value: impl Into<String>) -> Result<Self>;
    pub fn as_str(&self) -> &str;
    pub fn segments(&self) -> Vec<KeySegment>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillMode { Replace, Append }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SelectValue {
    Value(Option<String>),    // None == empty value
    Index(NonZeroU32),
    Label(NonEmptyText),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScrollDelta {
    ByOffset { dx: f64, dy: f64 },
    ToElement(ElementLocator),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DialogAction {
    Accept { prompt_text: Option<NonEmptyText> },
    Dismiss,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatedFilePath(String); // absolute, normalized, UTF-8, bounded

impl ValidatedFilePath {
    pub fn new(path: impl Into<String>) -> Result<Self>;
    pub fn as_str(&self) -> &str;
}
```

Request contracts (each with a validated constructor and Serde deserialization through `deserialize_validated`):

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClickRequest {
    pub target_id: TargetId,
    pub locator: InteractionLocator,
    pub button: MouseButton,
    pub modifiers: Modifiers,
    pub click_count: u8,           // 1..=3
    pub wait_for_navigation: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillRequest {
    pub target_id: TargetId,
    pub locator: InteractionLocator, // element required; coordinate rejected
    pub value: NonEmptyText,
    pub mode: FillMode,
    pub wait_for_navigation: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PressKeysRequest {
    pub target_id: TargetId,
    pub locator: Option<InteractionLocator>, // None == target-wide (current focus)
    pub keys: Vec<KeyChord>,                 // 1..=32
    pub wait_for_navigation: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectOptionRequest {
    pub target_id: TargetId,
    pub locator: InteractionLocator, // element required
    pub value: SelectValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoverRequest {
    pub target_id: TargetId,
    pub locator: InteractionLocator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DragRequest {
    pub target_id: TargetId,
    pub source: InteractionLocator,
    pub target: InteractionLocator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollRequest {
    pub target_id: TargetId,
    pub delta: ScrollDelta,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UploadFilesRequest {
    pub target_id: TargetId,
    pub locator: InteractionLocator, // element required
    pub files: Vec<ValidatedFilePath>, // 1..=8
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleDialogRequest {
    pub target_id: TargetId,
    pub action: DialogAction,
}
```

Constructors enforce: `ClickRequest::click_count` 1..=3; `FillRequest::locator` must be `Element` (coordinate rejected with `InvalidInput`); `PressKeysRequest::keys` non-empty and ≤32; `SelectOptionRequest::locator` must be `Element`; `UploadFilesRequest::locator` must be `Element`, files non-empty and ≤8; `ScrollDelta::ByOffset` finite values; `InteractionLocator::Coordinate { point, space }` validates the point is finite; `ValidatedFilePath` absolute, no `..` after normalization, ≤32 components, ≤4096 bytes UTF-8.

Interaction record, outcome, and result:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionOutcome {
    Dispatched,             // input dispatched and completion policy satisfied
    NoOpDetected,           // coordinate hit-test or pre-check found no actionable target
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SanitizedParameters(serde_json::Value);
impl SanitizedParameters {
    pub fn new(value: serde_json::Value) -> Result<Self>; // object, ≤4 KiB serialized
    pub fn as_json(&self) -> &serde_json::Value;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocatorSummary {
    pub kind: LocatorKind,            // reference | selector | coordinate_viewport | coordinate_document
    pub reference: Option<NodeReference>,
    pub selector_length: Option<u32>,
    pub coordinate: Option<CssPoint>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocatorKind {
    Reference,
    Selector,
    CoordinateViewport,
    CoordinateDocument,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InteractionRecord {
    pub id: InteractionId,
    pub context: ObservationContext,
    pub dispatch_time: SessionTime,
    pub live_observation_time: SessionTime,
    pub action: BrowserOperationKind,
    pub sanitized_parameters: SanitizedParameters,
    pub locator: LocatorSummary,
    pub outcome: InteractionOutcome,
    pub parent_batch: Option<InteractionId>, // always None for standalone
}
// Deserialization goes through a wire struct + validated constructor that
// re-checks ordering, action/locator consistency, and sanitized payload shape.

#[derive(Clone, Debug, PartialEq)]
pub struct InteractionResult {
    pub record: InteractionRecord,
    pub observation: LiveObservation,
}
```

Each interaction request implements:

```rust
pub trait BrowserActionRequest {
    fn locator(&self) -> Option<&InteractionLocator>;
    fn sanitize(&self) -> SanitizedParameters;
}
```

Sanitization rules:
- `Click`: `{button, modifiers, click_count, wait_for_navigation, locator: locator_summary}` (no value to redact).
- `Fill`: `{mode, value_length, wait_for_navigation, locator}` — never any value content.
- `PressKeys`: `{keys: [<chord.as_str()>], locator}` — keys are not sensitive.
- `SelectOption`: `{value: {kind, length}, locator}` — bound label/value length.
- `Hover`/`Drag`: `{locator, source, target}` summaries.
- `Scroll`: `{delta: {kind, ...}}`.
- `UploadFiles`: `{files: [<basename>], count, locator}` — basenames only, no directory or full path.
- `HandleDialog`: `{action, prompt_text_length}` — never the prompt text.

`LocatorSummary::from(locator)` produces the right `LocatorKind` and redacts selector content to length.

Add `ErrorCode::InteractionFailed` with `default_retry: Safe` and `default_recovery: Some("retry the action against a current target, or refresh the snapshot if the page has changed")`. Update `is_browser_session_failure`/`BROWSER_SESSION_CODES` to exclude it. Add `MAX_SANITIZED_PARAMETERS_BYTES = 4096`, `MAX_FILES_PER_UPLOAD = 8`, `MAX_PATH_BYTES = 4096`, `MAX_PATH_COMPONENTS = 32`, `MAX_KEY_CHORDS = 32`, `MAX_CLICK_COUNT = 3` as private `const`s.

`BrowserOperationRequest::target_id()` extends to cover the nine new variants; `BrowserOperationResult` gains nine new boxed-variant arms; `InteractionResult` is boxed uniformly like the observation results to keep enum layout small.

**Acceptance criteria:**

- [ ] One declaration generates all fourteen operation variants (5 observation + 9 interaction), result associations, stable names, mutability/evidence metadata, action metadata, and exhaustive registry tests including `action: None` for observation and `action: Some(&ACTION_*)` for interaction.
- [ ] Core remains runtime/transport/filesystem independent; only `serde_json` is reused for sanitized parameter values.
- [ ] Locators, modifiers, key chords, fill modes, select values, scroll deltas, dialog actions, validated file paths, click counts, key-chord counts, file counts, sanitized parameter size, interaction record ordering, and Serde round-trip validate at constructors and Serde boundaries.
- [ ] Each interaction request implements `BrowserActionRequest`; sanitization reduces sensitive payloads to non-sensitive metadata and never echoes fill values or CDP identifiers.
- [ ] Existing observation requests, registry tests, and Serde payloads continue to round-trip unchanged.

### Unit 2: Interaction dispatch foundation and pointer actions

**Files:**

- `crates/krometrail-cdp/src/control/interaction.rs` (new)
- `crates/krometrail-cdp/src/control/pointer.rs` (new)
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/ids.rs` (new — `UuidIdSource`)
- existing test fakes implementing `BrowserSessionPort`

**Story:** `epic-agent-browser-operation-verified-interactions-dispatch-and-pointer-actions`

The shared executor wraps every action in the same lifecycle:

```rust
pub(crate) struct InteractionPlan {
    pub kind: BrowserOperationKind,
    pub action: &'static ActionDefinition,
    pub locator: Option<InteractionLocator>,
    pub sanitized: SanitizedParameters,
}

impl PageControl {
    async fn execute_interaction(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        plan: InteractionPlan,
        started_at: SessionTime,
        dispatch: impl FnOnce(&ResolvedTarget, DispatchToken<'_>) -> InteractionDispatchFuture<'_>,
    ) -> Result<BrowserOperationResult>;
}

enum ResolvedTarget {
    Element { node: ResolvedNode },
    Coordinate { document_point: CssPoint, hit: Option<HitElement> },
    TargetWide,
}
```

`execute_interaction`:

1. Allocates `InteractionId` via `self.ids.next()`.
2. Resolves the locator: `Element` → `SnapshotRegistry::resolve`/`resolve_selector` with the action's `ReferenceRequirement`; `Coordinate` → fresh layout metrics + `Document.elementFromPoint` hit-test; `None` → `TargetWide`.
3. Builds a partial `InteractionRecord` with `dispatch_time` not yet set.
4. Calls the action-specific `dispatch` closure (which sends CDP input commands).
5. Applies the `CompletionKind`: `InputAcknowledged` returns immediately on the last command ack; `Settled` waits one bounded microtask checkpoint; `NavigationAware` subscribes to `Page.lifecycleEvent` (load/DOMContentLoaded) for a bounded window when `wait_for_navigation` is set.
6. Records `dispatch_time` and runs the same `inspect → snapshot → viewport-screenshot` sequence used by `observe_live`, producing a `LiveObservation`.
7. Sets `live_observation_time`, finalizes the `InteractionRecord` (`outcome: Dispatched` on success), and returns `InteractionResult { record, observation }`.

`SnapshotRegistry::resolve` gains the new `ReferenceRequirement` variants and richer `Runtime.callFunctionOn` facts. `ResolvedNode` exposes `backend_node_id` for downstream CDP calls (`DOM.setFileInputFiles`, `DOM.scrollIntoViewIfNeeded`). The resolver keeps exactly one round-trip per resolution; action-family code does not re-resolve.

Coordinate conversion reuses the screenshot clip conversion (`Page.getLayoutMetrics` → `cssLayoutViewport.pageX/pageY`). Hit-testing:

```javascript
(function (x, y) {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { tagName: el.tagName, x: r.left, y: r.top, width: r.width, height: r.height };
})(x, y)
```

with `throwOnSideEffect: true`, `returnByValue: true`, bounded result; null → `InteractionFailed` (`no_hit_target`).

`PageControl::new` signature:

```rust
pub(crate) fn new(
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    session_id: SessionId,
    session_origin: SessionOrigin,
) -> Self;
```

`ProductionBrowserConnector::new` defaults `ids` to `Arc::new(UuidIdSource)`; `with_capture` shares the capture assembly's source (mirroring the clock pattern). `connect` passes the source into `PageControl::new`. The supervisor's existing `SupervisorCommand::Execute` path is unchanged; `PageControl::execute` extends its `match` to route interaction variants to `execute_interaction` with the appropriate `dispatch` closure.

Pointer actions (`pointer.rs`) translate to `Input.dispatchMouseEvent`:

- **Click:** `mouseMoved` to point → `mousePressed` (button, clickCount, modifiers) → `mouseReleased`. Element-targeted uses the resolved quad center; coordinate uses the converted point. Modifiers map to the CDP `modifiers` bitmask (alt=1, ctrl=2, meta=4, shift=8).
- **Hover:** `mouseMoved` to point only. No press.
- **Drag:** `mouseMoved` → `mousePressed` → intermediate `mouseMoved` steps (a small fixed number of interpolations between source and target centers, ~5 steps) → `mouseReleased` at the target. Uses the standard CDP mouse event sequence Chrome accepts for native HTML5 drag.
- **Scroll:** `ByOffset` → `Input.dispatchMouseEvent` with `type: mouseWheel`, `deltaX`/`deltaY` (CSS pixels), at the current viewport center (or resolved element center for element-targeted scroll). `ToElement` → `DOM.scrollIntoViewIfNeeded({ backendNodeId })` first, then a no-op wheel so the post-action observation captures the scrolled state.

All `Input.dispatchMouseEvent` calls include `x`, `y` (document-space, converted to the visual-viewport-relative coordinate CDP expects using the current visual viewport `pageX/pageY`), `button`, `buttons` bitmask, `clickCount`, `modifiers`, and `pointerType: "mouse"`. Bounds and finite-ness are checked; non-finite converted coordinates return `InvalidInput`.

**Acceptance criteria:**

- [ ] `PageControl` carries an `IdSource`; connector plumbing shares the capture source or falls back to `UuidIdSource`; the supervisor command path remains single-writer and reconnect/stop paths answer queued interaction commands without dropping senders or replaying input.
- [ ] Element-targeted pointer actions resolve through the shared resolver with `Actionable`; coordinate-targeted actions perform one `Document.elementFromPoint` hit-test and fail `InteractionFailed` (`no_hit_target`) on empty hit.
- [ ] CDP `Input.dispatchMouseEvent` parameters are exact and finite for click/hover/drag/scroll; drag interpolates a bounded number of intermediate moves; element scroll uses `DOM.scrollIntoViewIfNeeded`.
- [ ] `Click`/`Hover`/`Drag` apply `Settled` completion; `Scroll` applies `InputAcknowledged`; `Click` honors `wait_for_navigation` by escalating to bounded `NavigationAware`.
- [ ] Every interaction returns an `InteractionResult` with a fully-populated `InteractionRecord` (id, context, dispatch/live-observation times, sanitized params, locator summary, `outcome: Dispatched`) and an honest partial `LiveObservation`.

### Unit 3: Keyboard and form actions

**Files:**

- `crates/krometrail-cdp/src/control/keyboard.rs` (new)
- `crates/krometrail-cdp/src/control/form.rs` (new)
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/tests.rs`

**Story:** `epic-agent-browser-operation-verified-interactions-keyboard-and-form-actions`

Keyboard actions (`keyboard.rs`):

- **Fill (`FillMode::Replace`):** focus the resolved element (`Input.dispatchMouseEvent` click at center, or `Input.focus` if cdpkit supports it via raw call), clear via `Input.dispatchKeyEvent` Ctrl+A + Delete (or select-all + Backspace), then `Input.insertText({ text: value })`. Fires input/change through the live DOM. `Append` mode skips the clear and inserts the text after the current cursor.
- **PressKeys:** for each `KeyChord` in the sequence, dispatch the parsed segments: `Modifier`/`NamedKey` become `rawKeyDown`/`keyUp` events with the CDP `key`/`code`/`windowsVirtualKeyCode` mappings; `Char` becomes `rawKeyDown` + `char` events. Modifier chords (e.g. `Control+S`) hold the modifier down across the named-key press and release it after. Element-targeted PressKeys focuses the element first.

CDP key mappings live in one private static table in `keyboard.rs`:

```rust
const KEY_DISPATCH: &[(&str, KeyDispatch)] = &[
    ("Enter", KeyDispatch { key: "Enter", code: "Enter", location: 0, keycode: 13 }),
    ("Tab", KeyDispatch { key: "Tab", code: "Tab", location: 0, keycode: 9 }),
    // ... full NamedKey set
];
```

`KeyChord::segments()` already validates the grammar in core; the adapter trusts the parsed segments and emits the corresponding events. Single Unicode chars dispatch through `Input.dispatchKeyEvent` with `text: <char>` and `Input.insertText` for composed text where appropriate.

Form actions (`form.rs`):

- **SelectOption:** resolve the element with `Selectable`, then set the value through a bounded `Runtime.callFunctionOn` that finds the matching `<option>` (by `value`, index, or visible label), sets `selected`, and dispatches `input`/`change` events. Reject if no option matches → `InvalidInput` (`select_value_not_matched`).
- The `<select>` setter runs under `throwOnSideEffect: false` (it intentionally mutates), but is bounded, returns a boolean success, and never exposes option text in the result beyond what `sanitize()` already chose.

`Fill`/`SelectOption`/`PressKeys` apply `Settled` completion (or `NavigationAware` when `wait_for_navigation` is set on `Fill`/`PressKeys`). The shared `execute_interaction` lifecycle is reused; only the `dispatch` closure differs.

**Acceptance criteria:**

- [ ] `Fill` replaces (Replace) or appends (Append) the value of an editable control via `Input.insertText` after focusing and clearing; non-editable elements fail at the resolver with `ReferenceNotActionable` (action-specific message).
- [ ] `PressKeys` dispatches validated key chords as `Input.dispatchKeyEvent` sequences, supports modifier chords, and accepts either an element locator (focus first) or `None` (target-wide current focus).
- [ ] `SelectOption` sets the matched option on a `<select>` through a bounded, side-effecting `Runtime.callFunctionOn` and dispatches `input`/`change`; non-`<select>` targets fail at the resolver; unmatched values fail `InvalidInput`.
- [ ] Keyboard/form actions carry honest completion and reuse the shared post-action `LiveObservation`; sanitized parameters reduce `Fill` values to character count only and bound `SelectOption` value length.

### Unit 4: File upload and dialog actions

**Files:**

- `crates/krometrail-cdp/src/control/upload.rs` (new)
- `crates/krometrail-cdp/src/control/dialog.rs` (new)
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/tests.rs`

**Story:** `epic-agent-browser-operation-verified-interactions-upload-and-dialog`

File upload (`upload.rs`):

- Resolve the locator with `FileInput` (action-specific resolver check); reject non-`<input type=file>` with `ReferenceNotActionable`.
- For each `ValidatedFilePath`, canonicalize at dispatch via `std::fs::canonicalize`, verify existence and read permission (`std::fs::File::open` succeed), and reject symlinks whose canonical target differs in component shape from the requested path. A path that fails canonicalization or read returns `InteractionFailed` (`upload_path_unreadable`) or `NotFound` (`upload_path_missing`) with the basename only in the error message.
- Send `DOM.setFileInputFiles({ files: ["/abs/..."], backendNodeId })` (CDP path mode). For multiple files, send all paths in one call.
- Apply `InputAcknowledged` completion; the post-action observation captures the input's `files` state via the snapshot.

Dialog actions (`dialog.rs`):

- Subscribe to `Page.javascriptDialogOpening` is **not** required; instead, probe current dialog state via a bounded `Runtime.callFunctionOn` that returns whether a dialog is currently open by inspecting Chromium's dialog-visible flag is **not** exposed to JS. Instead, the adapter dispatches `Page.handleJavaScriptDialog` and classifies the CDP response: success means a dialog was open and handled; the CDP error `"No dialog is showing"` maps to `NotFound` (`dialog_not_open`).
- `Accept { prompt_text }` sends `Page.handleJavaScriptDialog({ accept: true, promptText })`; `Dismiss` sends `{ accept: false }`. `prompt_text` is included only for `Accept`.
- Apply `InputAcknowledged` completion; the post-action observation captures the page state after the dialog was dismissed/accepted.

Both actions reuse the shared `execute_interaction` lifecycle. The `dispatch` closures send the action-specific CDP commands and return their result; the shared executor handles record finalization and observation.

**Acceptance criteria:**

- [ ] `UploadFiles` accepts only validated absolute normalized paths, canonicalizes at dispatch, verifies readability, rejects non-file-input targets at the resolver, and dispatches `DOM.setFileInputFiles` with all paths in one call; failure paths return `InteractionFailed`/`NotFound` with basename-only messages.
- [ ] `HandleDialog` dispatches `Page.handleJavaScriptDialog` with the right accept/promptText, classifies "no dialog" as `NotFound` (`dialog_not_open`), and never exposes dialog text in the result beyond the sanitized `prompt_text_length`.
- [ ] Both actions apply `InputAcknowledged` completion and reuse the shared post-action `LiveObservation`; the interaction record's sanitized parameters redact full file paths to basenames and the prompt text to length.

### Unit 5: Deterministic and real-browser qualification

**Files:**

- `crates/krometrail-cdp/tests/verified_interactions.rs` (new)
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs`
- `crates/krometrail-cdp/tests/support/mod.rs`
- `tests/fixtures/browser/verified-interactions/index.html` (new)
- `tests/fixtures/browser/README.md`

**Story:** `epic-agent-browser-operation-verified-interactions-qualification`

Extend the existing scripted raw transport rather than introducing a second fake protocol stack. Deterministic tests assert:

- Exact `Input.dispatchMouseEvent` JSON for click/hover/drag/scroll, including modifier bitmask, clickCount, and document→visual viewport coordinate conversion.
- Element actionability routing: `Actionable` for click/drag-source, `VisibleGeometry` for hover, `Editable` for fill, `Selectable` for select, `FileInput` for upload — each verified by feeding a node-state response that satisfies or violates the requirement and asserting the right stable code.
- Coordinate hit-test: `Document.elementFromPoint` returns null → `InteractionFailed` (`no_hit_target`); non-null → dispatch proceeds.
- Stale reference during interaction: snapshot generation replaced between snapshot and click → `StaleReference`.
- Navigation-aware completion: `wait_for_navigation: true` consumes a `Page.lifecycleEvent` event within the bounded window; timeout without the event still resolves the action successfully (post-action observation captured) but the timing is recorded.
- Key chord translation: `KeyChord` parsing produces the right `Input.dispatchKeyEvent` sequence for `Enter`, `Control+S`, `Shift+ArrowDown`, and a multi-char string; malformed chords reject at construction in core (unit-tested) and never reach the adapter.
- Fill modes: `Replace` clears then inserts; `Append` skips clear.
- Select value matching: value/index/label each set the right option; unmatched label → `InvalidInput`.
- File upload: valid path dispatches `DOM.setFileInputFiles`; missing path → `NotFound`; non-file-input target → `ReferenceNotActionable`.
- Dialog: `Accept`/`Dismiss` produce the right `Page.handleJavaScriptDialog` payload; "no dialog" CDP error → `NotFound`.
- Sanitization: each action's `SanitizedParameters` redacts the right fields (Fill value, dialog prompt, upload paths) and never echoes CDP identifiers, backend node ids, object ids, or transport session ids.
- Interaction record: id allocated from `IdSource`, dispatch/live-observation times ordered, locator summary kind matches, `parent_batch: None`, `outcome: Dispatched`.
- Reconnect/stop completion: queued interaction commands during reconnect receive `BrowserDisconnected` without replay; queue closure receives `Cancelled`.

Add one dependency-free interaction fixture with: normal/disabled/hidden buttons, text input + textarea + checkbox + `<select>` + `<input type=file>` + contenteditable, a draggable element with known HTML5 drag handlers, a scrollable container with known dimensions, a coordinate-clickable `<div>` with known position, and a button that opens `confirm()`/`prompt()` dialogs. Opt-in real Chrome tests use the production connector and operation port to verify:

1. Click triggers a visible DOM change and the post-action snapshot reflects it.
2. Fill (Replace and Append) updates the input value and fires `change`.
3. PressKeys types into a focused field and dispatches a modifier chord.
4. SelectOption sets value/index/label.
5. Hover triggers a `:hover` style change verifiable via `Runtime.evaluate`.
6. Drag moves a draggable element to a known target.
7. Scroll-by-offset and scroll-to-element change scroll position.
8. UploadFiles sets a file input's files (using a temp file created by the test).
9. HandleDialog accepts and dismisses `confirm()`/`prompt()`.
10. Coordinate click on empty space fails `no_hit_target`; coordinate click on a known div succeeds.
11. Stale reference after a snapshot refresh and after dynamic replacement.

The real-browser test is opt-in under `KROMETRAIL_REAL_CHROME_TESTS=1`; deterministic contract coverage remains in the default suite. The fixture is target-only, dependency-free, documented in `tests/fixtures/browser/README.md`, and introduces no second Krometrail runtime.

**Acceptance criteria:**

- [ ] Default deterministic tests protect the stable action/reference/error/sanitization seams without depending on Chrome timing.
- [ ] Production-connector Chrome tests cover click/fill/press/select/hover/drag/scroll/upload/dialog and the coordinate-fallback + stale-reference boundaries on Linux; platform/scale observations remain explicit.
- [ ] The fixture is target-only, dependency-free, documented, and introduces no second Krometrail runtime.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass.

## Error and actionability semantics

| Condition | Stable code | Retry | Recovery |
| --- | --- | --- | --- |
| Reference target/generation/document/backing node expired during interaction | `stale_reference` | after recovery | request a new structured snapshot and retry with its reference |
| Element hidden, inert, disabled, detached, or geometry-less | `reference_not_actionable` | after recovery | refresh the snapshot or choose another target |
| Element actionable but not editable / not a `<select>` / not a file input | `reference_not_actionable` | after recovery | refresh the snapshot or choose an action-appropriate target |
| Coordinate action hit empty space | `interaction_failed` (`no_hit_target`) | safe | scroll/pan or pick a target with an element at the coordinate |
| Select value did not match any option | `invalid_input` | never without correction | choose a value/index/label that exists |
| Malformed key chord, click count, fill locator form, or upload file count | `invalid_input` | never without correction | correct the request |
| Upload path missing or unreadable at dispatch | `not_found` / `interaction_failed` | safe (after correction) | provide an existing readable file |
| No JavaScript dialog open | `not_found` (`dialog_not_open`) | safe | open the dialog first or omit the action |
| CDP input rejection, completion timeout, or malformed input response | `interaction_failed` | safe for `InputAcknowledged`; never for `Settled`/`NavigationAware` whose effect may have partially applied | retry against a current target, or refresh the snapshot |
| Unknown/closed target | `not_found` or `target_failed` | safe only when target still exists | refresh/list targets and choose an attached page |
| Session reconnecting during interaction | `browser_disconnected` | after recovery | wait for ready status, then repeat the action |
| Stop/end during interaction | `cancelled` | after recovery | restart the operation if still needed |

Raw CDP response text, endpoint, browser target key, transport session id, object id, backend node id, key code tables, and dialog text never enter info logs or stable error messages.

## Implementation order

1. `epic-agent-browser-operation-verified-interactions-core-contracts`
2. `epic-agent-browser-operation-verified-interactions-dispatch-and-pointer-actions`
3. `epic-agent-browser-operation-verified-interactions-keyboard-and-form-actions`
4. `epic-agent-browser-operation-verified-interactions-upload-and-dialog`
5. `epic-agent-browser-operation-verified-interactions-qualification`

One feature owner should normally carry all five checkpoints. The dependencies expose the contract and verification order; they are not five parallel worker assignments.

## Simplification

- Extend the existing `define_browser_operations!` macro and `BROWSER_OPERATION_REGISTRY` slice with optional action metadata instead of a parallel action enum, schema, or service.
- Extend the existing shared resolver (`SnapshotRegistry`) with action-specific `ReferenceRequirement` variants and one richer `Runtime.callFunctionOn` rather than adding per-action resolver bypasses.
- Reuse the existing `observe_live` inspect/snapshot/screenshot sequence for post-action evidence; do not fork the live-observation contract.
- Reuse `CdpTransport::send_raw`, `ElementLocator`, `CoordinateSpace`, `CssPoint`, `CssRect`, `quad_bounds`, layout-viewport conversion, stable core errors, and the bounded capture image-header reader.
- Reuse the existing single-writer production supervisor, exact transport-session binding, and reconnect/stop completion paths; do not add a browser-control session map, action queue, or active-page registry.
- Keep selector and coordinate targeting as explicit weaker request forms, not identities or compatibility aliases; coordinates never mint references.
- No test is proposed for trivial getters, every CDP key mapping, or every select option. No old contract needs a compatibility shim because interaction types are net-new.

## Testing

- **Core interface tests:** Exhaustively prove the extended macro registry, request/result association, action metadata, and Serde round-trip; protect validation for locators, modifiers, key chords, fill modes, select values, scroll deltas, dialog actions, validated file paths, sanitized parameters, interaction-record ordering, and the new error code. These are public contract risks.
- **Scripted adapter tests:** Protect exact CDP input JSON for each action family, actionability routing per `ReferenceRequirement`, coordinate hit-test conversion and no-op refusal, stale-reference detection during interaction, navigation-aware completion, file-path dispatch with rejection paths, dialog classification, sanitization redaction, interaction-record allocation/timing, and reconnect/stop completion without replay. These are deterministic failure and concurrency risks.
- **Opt-in real Chrome test:** Protect the assumptions scripted JSON cannot establish: actual click/fill/select/hover/drag/scroll/upload/dialog behavior, HTML5 drag, contenteditable, file inputs, JavaScript dialogs, coordinate fallback, and stale-reference after dynamic replacement.
- **Test removal/consolidation:** Extend the existing `ScriptedCdp` and Chrome wrapper helpers rather than adding interaction-only fakes for the same seams. Reuse the existing `page-observation` fixture primitives (layout, ax-tree, frame-tree, png helpers) where they apply.

## Risks

- **HTML5 drag fidelity:** Chrome's native drag requires a precise `Input.dispatchMouseEvent` sequence and may not always trigger `dragstart`/`drop` reliably across platforms. The deterministic test asserts the exact event sequence; the real-Chrome test proves the supported envelope on Linux. If a real browser rejects the synthesized sequence, the fallback is to expose drag as a documented best-effort pointer action and record the boundary; native drag-and-drop of files into the page belongs to a future drag-files extension.
- **Key chord coverage:** The closed `KeyChord` grammar covers common automation keys, but exotic keys (media, IME composition, dead keys) are out of scope. Unknown multi-char tokens reject explicitly; single Unicode characters dispatch through `Input.dispatchKeyEvent`. IME and dead-key composition are explicit future work.
- **Navigation-aware completion:** `Page.lifecycleEvent` may not fire for SPA-style navigations that change the URL via `history.pushState`. The bounded `NavigationAware` window resolves successfully on timeout (the action still completed) but the timing record is honest. True SPA navigation detection is a future waits-and-batches concern.
- **Select via `Runtime.callFunctionOn`:** Setting `<select>` value through JS is the most reliable cross-browser path, but unusual custom select widgets (role=combobox without a native `<select>`) will not match `Selectable` and fail at the resolver. That boundary is explicit and recorded.
- **File upload path canonicalization:** `std::fs::canonicalize` resolves symlinks; a path that exists but is a symlink to a different absolute form is accepted at its canonical target. Local-agent authority is the local user, so this is honest behavior, but the canonical form is what reaches Chrome, not the literal requested string.
- **Dialog detection:** CDP does not expose a "is a dialog currently open" query; the adapter classifies based on the `Page.handleJavaScriptDialog` response. Race conditions where a dialog closes between observation and dispatch are bounded by `InputAcknowledged` completion and a stable `not_found` if the dialog is gone.
- **Upload payload size:** Large file uploads stream from disk through CDP; the bounded base64/decoded limits from the screenshot path do not apply (paths, not bytes). A pathological file count or size is bounded by `MAX_FILES_PER_UPLOAD` and the local filesystem; unbounded upload is a future MCP/streaming concern.
- **Interaction record persistence:** This feature creates records in memory and returns them in results; durable persistence is owned by `epic-durable-browser-memory`. The record contract is stable so the future sink can consume it without renegotiation.

## Pre-mortem

The riskiest assumption is that the synthesized `Input.dispatchMouseEvent` sequence drives real Chrome interactions — particularly HTML5 drag and select-all-then-delete for fill clear — the way agents expect. Failure would show up as scripted tests passing (exact JSON) while real-Chrome tests show inputs that "dispatched" but had no page effect. The design prevents the dangerous form by always capturing an honest post-action `LiveObservation`: even if the page did not change, the agent sees the actual resulting state and can retry. The real-Chrome qualification checkpoint is the integration guard; if HTML5 drag fails, drag is documented as best-effort and the boundary is recorded, not silently shipped. The area of least certainty is `NavigationAware` completion for SPA navigations, so completion defaults to `Settled` and `NavigationAware` is opt-in per request.

## Architectural note for the implementor

- The shared `execute_interaction` executor is the central abstraction. Action-family modules provide only the CDP `dispatch` closure and any action-specific helpers; they must not re-resolve references, re-implement coordinate conversion, re-capture the live observation, or re-allocate the interaction id. Violating this re-introduces the per-action architecture Option B was rejected for.
- The `Runtime.callFunctionOn` fact set in `validate_node_state` must remain the single actionability source of truth. Action-family code that needs more DOM facts (e.g. option existence for `SelectOption`) makes additional bounded calls rather than extending this fact set unless the new fact is genuinely cross-action.
- The interaction record is built across the lifecycle (id allocated → partial record → dispatch → completion → observation → finalized record). Implementation must not lose the partial record on an error path: pre-dispatch errors return the stable actionability/stale error without a record (no dispatch occurred); post-dispatch errors that leave the page in an unknown state still capture a partial `LiveObservation` and an `outcome: Dispatched` record when the input was sent, so the agent has honest evidence rather than a missing result. Cancellation during dispatch follows the supervisor's existing queue-closure path.

## Implementation roll-up (2026-07-14)

All five implementation checkpoints are complete and the feature is ready for review. The qualification checkpoint is recorded in commit `1407099`.

- Initial fresh attachments and reconnects share the ordered session-domain restoration step (`Page.enable`, `Runtime.enable`, `Accessibility.enable`) before visibility probing.
- Dialog dispatch is exact-session and single-attempt; speculative retries, renderer checkpoints, pending-dialog state, and debug scaffolding were removed.
- Real Chrome qualification passed all eight verified-interactions tests under `KROMETRAIL_REAL_CHROME_TESTS=1`; deterministic qualification passed eight tests.
- CDP package gates passed: 190 all-target tests, no-default-features check, and Clippy with `-D warnings`.
- Workspace check and tests passed (379 tests). Workspace Clippy remains blocked by unrelated concurrent range-test warnings only; no range files were modified or staged.

## Review bounce 1 (2026-07-14)

**Verdict:** Request changes

**Receiver-confirmed blockers:**

1. Element-targeted pointer resolution currently treats `DOM.getBoxModel` quad centers as viewport coordinates, while screenshot and declared document-coordinate paths treat the same geometry as document/page coordinates. Existing tests only click before scrolling and never assert dispatched coordinates with non-zero visual-viewport offsets. Add deterministic non-zero-offset dispatch assertions plus a real-Chrome scroll-then-click target, then correct or empirically justify the conversion.
2. Fill sanitization persists the first 32 characters verbatim, exposing complete short passwords, tokens, and codes. Replace value previews with non-sensitive metadata (for example, character count) and prove short values never appear.

**Important to adjudicate in remediation:** A dialog event can win `tokio::select!` and drop an in-flight multi-command pointer dispatch between press and release. Verify cdpkit abandonment and pointer-state behavior with a focused test or restructure the boundary so dialog observation does not leave a partially dispatched gesture.

**Non-blocking:** Restore or explain `throwOnSideEffect` for coordinate hit testing; align final design prose with shipped field names; sharing one layout-metrics read for drag is a performance nit outside this correction.

The approved initial-attach domain restoration, exact-session single-attempt dialog handling, registry/actionability/key/upload/error/record contracts, and package gates remain valid. This bounce is focused on post-scroll pointer correctness, secret-safe fill records, and the dispatch-cancellation edge above.

## Review bounce 1 remediation (2026-07-14)

**Dispatch:** One feature-remediation owner, as delegated by autopilot. The accepted findings share the interaction executor and qualification surface, so splitting them would add handoff risk without independent write ownership. The already-consumed standard review pass was not repeated.

**Coordinate adjudication:** The existing element-pointer conversion is correct for Chrome and is now qualified rather than changed. A direct real-Chrome probe after `scrollY = 700` returned a `DOM.getBoxModel` border with `y = -620`, exactly matching `getBoundingClientRect().y = -620`, while `cssVisualViewport.pageY = 700`. The quad is therefore already viewport CSS geometry for `Input.dispatchMouseEvent`; subtracting the visual-viewport page offset would double-apply scrolling and mis-aim the action. `verified_interactions.rs` now scripts `pageX = 400` / `pageY = 900`, supplies a box center at `(170, 100)`, and asserts all three click events retain `(170, 100)`. The real fixture adds a target at document `top = 1200`; Chrome scrolls it into view, reports non-zero `window.scrollY`, and the subsequent element-targeted click changes fixture state.

**Secret-safe records:** `FillRequest::sanitize` no longer emits `value_preview` or any fill content. It persists only mode, character count, navigation intent, and the redacted locator summary. Core and adapter-contract regressions cover a short password, token, and numeric code and assert both that each value is absent and that the preview field does not exist.

**Dialog/cancellation adjudication:** The concern was valid. cdpkit 0.4.0 inserts a pending response sender, queues the WebSocket command, and awaits its response; dropping that future does not retract the command, and the reader later removes the pending entry when the response arrives. With the prior sequential click/drag awaits, `Page.javascriptDialogOpening` could therefore win after `mousePressed` had been queued but before `mouseReleased` was created. Click and drag now eagerly poll their bounded stateful command group together (press, moves, release) after the initial stateless move. The focused regression holds all stateful pointer responses pending, abandons the dispatch future as the dialog branch would, and proves `mouseReleased` was already queued. This preserves dialog non-deadlocking without leaving a partially dispatched gesture.

**Rejected non-blocking proposal:** Coordinate hit testing intentionally does not set `throwOnSideEffect`. The expression is fixed adapter-owned code, returns by value, and only calls `elementFromPoint`/`getBoundingClientRect`; Chromium's side-effect analyzer may conservatively reject such DOM calls, which would turn valid coordinate targeting into a protocol failure without adding a security boundary. The hit-test remains bounded and accepts no executable caller input. The shipped sanitization field names above replace the superseded preview prose; drag layout-read sharing remains a performance nit outside this correction.

**Files changed:**

- `crates/krometrail-core/src/browser/interaction.rs`
- `crates/krometrail-cdp/src/control/pointer.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/verified_interactions.rs`
- `tests/fixtures/browser/verified-interactions/index.html`
- `.work/active/features/epic-agent-browser-operation-verified-interactions.md`

**Verification:**

- `cargo test -p krometrail-core browser::interaction --locked` — 3 passed.
- `cargo test -p krometrail-cdp --lib control::tests::interactions --locked` — 4 passed.
- `cargo test -p krometrail-cdp --test verified_interactions --locked` — 9 passed.
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test verified_interactions --locked -- --nocapture` — 9 passed, including the non-zero-scroll element click against real Chrome.
- `cargo fmt --all -- --check` — passed.
- `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` — passed.
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — 381 passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.

Implementation returned to `stage: review` for caller-managed feature closure. No second independent review was run under the standard-weight policy.

## Review closure (2026-07-14)

**Verdict**: Approve

**Blockers**: none — both receiver-confirmed blockers from the single standard pass were fixed and verified.
**Important**: none — the pointer-dispatch cancellation proposal was confirmed material and corrected in the same remediation.
**Nits**: drag layout-read sharing remains a performance nit outside this correction.
**Rejected**: Adding `throwOnSideEffect` to the fixed, adapter-owned coordinate hit test was rejected because Chromium can conservatively refuse its bounded DOM reads without creating a meaningful security boundary.

**Notes**: Standard-weight closure used fix verification only; the independent pass was not repeated. The receiver inspected commit `6efcbaf`, confirmed element box-model coordinates are qualified as viewport-relative under non-zero scroll, fill records retain only character counts, and stateful click/drag commands queue release before dialog-driven abandonment can split the gesture. Fresh focused verification passed 3 core interaction tests, 4 CDP interaction tests, and 9 verified-interactions contract tests. The implementation worker additionally reported real-Chrome qualification (9 tests), workspace check/test (381 tests), and workspace Clippy with warnings denied as green.
