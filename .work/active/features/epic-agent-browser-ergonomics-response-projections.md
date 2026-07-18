---
id: epic-agent-browser-ergonomics-response-projections
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

# Agent-sized response projections

## Brief

Add one validated MCP presentation preference for browser operations and temporal entry points, plus a concise `browser_status` detail mode. Callers can independently request inline images or select legacy/full/compact/omitted structured observations where the existing projector has a truthful representation. Omitted preferences choose the economical compact server projection; underlying action observation, retention, warnings, diagnostics, and canonical resources remain unchanged, while explicit `legacy`, `full`, and `inline` expand presentation when needed.

This feature does not introduce persistence for live screenshots or skip required post-action observation. It teaches the Krometrail skill to request the cheapest sufficient projection and to drill into explicit snapshot, screenshot, status, or resource tools only when needed.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent MCP presentation contract used by routine agent workflows

## Simplification opportunity

Extend the shared response projector and lifecycle argument schema rather than adding compact variants per tool or duplicating `BrowserStatus` in the domain.

## Foundation references

- `docs/SPEC.md` — Current-State Observation
- `docs/ARCHITECTURE.md` — MCP Boundary

## Design decisions

- **Projection ownership**: keep response preferences in `krometrail-mcp`; remove the additive presentation field before decoding the unchanged core request and apply it after the authoritative result is acquired — presentation cannot influence browser dispatch, temporal acquisition, retention, warnings, or interaction identity.
- **Omitted preference behavior**: an absent `response` object selects compact snapshot/page-state detail and omits inline image bytes. Explicit `legacy` retains the earlier automatic snapshot presentation; explicit `full` and `inline` expand the requested structures and image content. This intentional minor-release default change follows the user-directed agent-ergonomics contract while preserving authoritative result, warning, interaction, retention, and resource identities.
- **Live images**: support `inline` and `omit`, not `resource`, because live post-action screenshots have no canonical retained resource authority. Omission removes the MCP image content block but retains screenshot availability metadata.
- **Structured parts**: snapshot and page-state preferences are independent. `compact` uses deterministic truthful summaries; `omit` emits an explicit `{ "omitted": { "reason": "response_projection" } }` observation part rather than making acquired evidence look unavailable.
- **Diagnostics**: diagnostics remain automatic for failed or degraded results unless explicitly omitted. Successful results remain diagnostic-free; projection cannot request sensitive diagnostic content.
- **Status detail**: `browser_status` accepts `detail: "concise" | "full"`, defaulting to `concise`. Start/attach retain their full status result; agents request full status only for compatibility and timing diagnostics.
- **Compatibility**: decorate generated input schemas from the same Rust projection contract and continue decoding every underlying request through its existing validated wire type. Do not add compact tool aliases or change any core operation request.
- **UI surface**: none; this is an MCP and skill-instruction surface, so no mockup is required.

## Architectural choice

Three approaches were considered. Putting preferences into every core browser and temporal request would make presentation affect the inward domain boundary and multiply otherwise identical contracts. Adding separate `*_compact` tools would duplicate the registry, schemas, annotations, and skill guidance. The chosen approach is one MCP-owned request decorator plus one shared response projector: schemas advertise an optional `response` property, routing extracts it before existing request decoding, and the response layer projects the already-computed result. It preserves validated-wire and registry-declared-surface patterns while keeping omitted fields exactly compatible.

The highest-risk unit is deterministic compact projection of nested live observations and temporal bundles. It must reduce model payload without losing action outcome, interaction anchor, warning/degradation state, screenshot metadata, resource identities, or temporal provenance. It is designed before route integration so every route shares one tested transformation.

## Implementation Units

### Unit 1: Validated response preference and schema decoration

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`
**Story**: `epic-agent-browser-ergonomics-response-projections-projector`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StructuredResponseDetail {
    Legacy,
    Full,
    #[default]
    Compact,
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InlineImageDetail {
    Inline,
    #[default]
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticDetail {
    #[default]
    Automatic,
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResponseProjectionRequest {
    #[serde(default)]
    pub inline_images: InlineImageDetail,
    #[serde(default)]
    pub snapshot: StructuredResponseDetail,
    #[serde(default)]
    pub page_state: StructuredResponseDetail,
    #[serde(default)]
    pub diagnostics: DiagnosticDetail,
}

pub(crate) fn projected_input_schema(
    base: Arc<JsonObject>,
) -> Result<Arc<JsonObject>>;

pub(crate) fn split_response_projection(
    arguments: JsonObject,
) -> Result<(JsonObject, ResponseProjectionRequest)>;
```

**Implementation notes**:

- `projected_input_schema` inserts an optional `response` property generated from `ResponseProjectionRequest`; it preserves the base root's existing `required` and `additionalProperties: false` contract and fails closed if the expected object shape is absent.
- `split_response_projection` removes only `response`, decodes it with `serde_path_to_error`, and returns the remaining object for the existing core decoder. Unknown preference fields and invalid enum values produce the existing sanitized `invalid_input` envelope without echoing supplied values.
- `Legacy` is distinct from `Full`: legacy retains existing post-action automatic snapshot compaction, while explicit full includes the acquired full snapshot. This avoids silently changing 1.x output while making the new option names truthful.

**Acceptance criteria**:

- [ ] Omitted `response` preserves the same advertised underlying request fields while selecting the economical compact/no-inline presentation; explicit `legacy`, `full`, and `inline` remain available.
- [ ] Every projected browser and temporal entry schema advertises the same dereferenced, closed `response` object; invalid nested values fail at their normalized field path without content disclosure.
- [ ] Batch applies one outer response projection to its final observation and step images; batch step request schemas remain the underlying standalone operation schemas and do not gain nested projection controls.

### Unit 2: Shared result projection and compact representations

**File**: `crates/krometrail-mcp/src/response.rs`
**Story**: `epic-agent-browser-ergonomics-response-projections-projector`

```rust
pub(crate) fn map_operation_result_with_capture(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[TargetCaptureStatus],
    preference: ResponseProjectionRequest,
) -> Result<MappedResult, ResponseInvariantError>;

pub(crate) async fn map_temporal_bundle_result(
    tool: &str,
    bundle: TemporalDebugBundle,
    store: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
    preference: ResponseProjectionRequest,
) -> Result<MappedResult, ResponseInvariantError>;

fn apply_response_projection(
    projection: &mut Projection,
    preference: ResponseProjectionRequest,
) -> Result<(), ResponseInvariantError>;

fn compact_page_state(value: &Value) -> Result<Value, ResponseInvariantError>;
fn compact_snapshot_value(value: &Value) -> Result<Value, ResponseInvariantError>;
fn compact_temporal_value(value: &Value) -> Result<Value, ResponseInvariantError>;
```

**Implementation notes**:

- Perform existing result validation and resource discovery first, then project. Never mutate `status`, `interaction`, `warnings`, `error`, or `resources`.
- `inline_images: omit` clears encoded image content blocks and `images` metadata entries only after preserving screenshot/artifact availability metadata in `result`; canonical resource links remain.
- Snapshot `compact` reuses the existing actionable-node/ancestor selection and byte/node ceilings. `full` bypasses automatic snapshot compaction. `omit` retains the surrounding availability state with the explicit projection-omitted marker.
- Page-state `compact` retains target/session identity, URL/title, selection, viewport/effective geometry, navigation/dialog state, and observation time while removing verbose ancillary structures only when those named fields exist. Unknown result shapes are left unchanged rather than guessed.
- Temporal `compact` retains requested/resolved timing, header summary, effective policy identity, warnings, degradations, artifact handles, frame/resource handles, gaps, and event-count summaries; it omits repeated full range copies and verbose nested evidence rows already addressable through resources/focused tools.
- Projection runs in one place for operation, bundle, progressive, context, video, and lifecycle mappings. Tool-specific code may supply a compact serializer but must not implement its own preference switch.

**Acceptance criteria**:

- [ ] Compact and omit modes never change operation status, interaction identity, warnings, errors, capture-failure degradation, resource URIs, or retained-service calls.
- [ ] Explicit full returns a full acquired post-action snapshot; legacy retains the current 96-node/32-KiB automatic behavior.
- [ ] Omitted inline images create no MCP image content blocks and materially bound a representative mutation/bundle response while leaving truthful image availability metadata.
- [ ] A temporal bundle that previously overflowed agent context can be requested without inline images and with compact structures while preserving the handles needed to drill down.

### Unit 3: Concise browser-status projection

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/registry.rs`
**Story**: `epic-agent-browser-ergonomics-response-projections-route-integration`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserStatusDetail {
    #[default]
    Concise,
    Full,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserStatusRequest {
    #[serde(default)]
    pub detail: BrowserStatusDetail,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ConciseBrowserStatus {
    pub session_id: SessionId,
    pub state: BrowserSessionState,
    pub ownership: BrowserOwnership,
    pub profile: ProfileRef,
    pub selected_target_id: Option<TargetId>,
    pub page_count: u32,
    pub capture: Vec<ConciseCaptureStatus>,
    pub retention: ConciseRetentionStatus,
    pub every_nth_frame: EveryNthFrame,
}

pub(crate) fn map_browser_status(
    tool: &str,
    status: BrowserStatus,
    detail: BrowserStatusDetail,
) -> Result<MappedResult, ResponseInvariantError>;
```

**Implementation notes**:

- Concise per-target capture retains target ID, stream state, received/persisted/dropped counts, known gap count, last frame time, and failure stage. Concise retention retains used, configured, and pinned bytes plus budget/recording-blocked state.
- Do not create a second domain status model or recalculate evidence health from ordinal gaps. The response types serialize selected fields from validated `BrowserStatus` and `TargetCaptureStatus`.
- `browser_status {}` is concise. `browser_status {"detail":"full"}` expands to the complete existing status value; stop remains an empty object.

**Acceptance criteria**:

- [ ] Concise status answers active session, selected target, page count, per-target capture health/loss, retention pressure, and cadence without compatibility matrices or timing distributions.
- [ ] Explicit full status remains serialization-equivalent to the complete `BrowserStatus` response.
- [ ] Failed capture and paused-budget states remain visible in concise output.

### Unit 4: Route wiring, diagnostics policy, schemas, and agent guidance

**Files**: `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/server.rs`, `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/test_fixture.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`
**Story**: `epic-agent-browser-ergonomics-response-projections-route-integration`

```rust
fn requested_diagnostic_detail(
    request: &CallToolRequestParam,
) -> DiagnosticDetail;

fn attach_diagnostics(
    result: &mut CallToolResult,
    correlation_id: &str,
    context: &DiagnosticContext,
    detail: DiagnosticDetail,
) -> &'static str;
```

**Implementation notes**:

- Browser operation, bundle, progressive, browser-event, and video routes all call `split_response_projection` before their existing typed decode and pass one preference to response mapping. Start/attach remain unchanged; status uses its own detail request.
- The server reads only the narrow `response.diagnostics` enum from the original request before routing. Invalid projection values still fail in the route; the server defaults to automatic diagnostics if it cannot safely recognize a valid omission request.
- `diagnostics: omit` suppresses the response correlation/path only; structured warning/error fields and sanitized tracing remain unchanged. MCP protocol-level errors keep diagnostics because they did not pass a valid tool preference boundary.
- Update the skill to request `inline_images: omit` and compact snapshot/page state for routine mutations, `browser_status {"detail":"concise"}` for capture checks, and full/narrow follow-up tools only when the compact projection is insufficient.
- Regenerate canonical MCP/tool schema fixtures through the existing generator path; never hand-edit generated artifacts.

**Acceptance criteria**:

- [x] Registry validation proves every intended route has exactly one projection decoration and no duplicate compact route exists.
- [x] Error/degraded responses attach diagnostics by default, suppress them only after a valid explicit omission, and never expose request values.
- [x] Plugin guidance gives copyable economical request examples and still directs agents to full evidence/resources for strong claims.
- [x] Exact generated schema/fixture tests and an stdio round trip cover omitted, compact, full, invalid, and legacy requests.

## Implementation order

1. `epic-agent-browser-ergonomics-response-projections-projector`: preference contract, schema decorator, shared projector, and payload-bound regression tests.
2. `epic-agent-browser-ergonomics-response-projections-route-integration`: concise status, route/server integration, generated schema checks, and skill guidance.

## Simplification

- Replace tool-specific future compaction switches with one projection vocabulary and one post-acquisition application point.
- Reuse `compact_automatic_snapshot` as the sole actionable-node selection algorithm; rename it to reflect shared use rather than adding a second compact snapshot implementation.
- Keep the one `ToolResponse` envelope and existing resource projector. No compact response envelope, lifecycle status model, or live-screenshot store is added.
- Retain existing legacy response tests as compatibility fixtures; remove only assertions duplicated by the new schema-wide registry test.

## Testing

- One response-layer table test protects the stable combinations across mutation, batch, screenshot, bundle, and resource-bearing results; it asserts invariant fields and content-block counts instead of snapshotting incidental JSON order.
- One schema registry test protects additive closed-schema decoration and legacy required fields for all routes.
- One stdio integration sequence protects actual request extraction, full core validation, concise status, diagnostic omission, and legacy defaults.
- One representative large snapshot and bundle regression protects the observed context-overflow risk with explicit serialized-byte ceilings. It uses deterministic fixture data, not a real browser.
- Existing full-response, live-observation degradation, capture warning, and canonical resource tests remain the authority and must continue unchanged.

## Risks

- The riskiest assumption is that compact representations can be truthful across heterogeneous operation results. The fallback is to leave unrecognized shapes in legacy form and compact only named, tested live-observation and temporal structures.
- Removing image content blocks can surprise callers that inspect `images` rather than `result`; explicit omission therefore removes both while preserving availability metadata and resources.
- Diagnostics are attached outside route mapping. Reading the narrow preference at the server boundary must fail toward automatic diagnostics, never toward accidental suppression.
- Additive schema rewriting can drift from deserialization. The same `ResponseProjectionRequest` schema and `split_response_projection` decoder, exercised across the registry, are release-critical.

## Implementation notes

- Execution capability: one sequential inline feature owner; the projector and route integration shared response invariants and were completed as two ordered child checkpoints.
- Review weight: standard (project default); the feature is intentionally left at `review` for the caller's independent feature review.
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/server.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`.
- Tests added/removed: added validated projection/schema tests, payload-bound tests, concise status tests, diagnostic-policy tests, and stdio/default-expansion coverage; removed none.
- Simplification: one response vocabulary, one schema decorator, one request splitter, one post-acquisition projector, and one `BrowserStatus` projection serve the complete route surface.
- Discrepancies from design: the user explicitly changed omitted preferences to economical compact/no-inline output and omitted status detail to concise output. Explicit `legacy`, `full`, and `inline` retain expansion paths; generated operation roots remain open exactly as before while the nested response object is closed.
- Adjacent issues parked: none.

## Integrated verification

- All child stories are `stage: done`.
- `cargo test -p krometrail-mcp --locked` (52 passed).
- `cargo check -p krometrail-mcp --all-targets --locked`.
- `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings`.

## Review (2026-07-18)

**Verdict**: Approve

**Blockers**: The one standard-pass finding was accepted and fixed: lifecycle tools that do not
advertise response projection can no longer suppress diagnostics by supplying an invalid
`response.diagnostics: omit` argument.
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Substrate feature review at standard weight, one balanced independent pass. The accepted
diagnostic-boundary blocker was corrected in `83c1def` and verified with an in-memory MCP regression
for invalid `browser_status` input plus focused MCP clippy with warnings denied. Per the standard
closure policy, the verified fix closes the feature without a second independent pass.
