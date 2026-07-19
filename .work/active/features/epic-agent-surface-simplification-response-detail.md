---
id: epic-agent-surface-simplification-response-detail
kind: feature
stage: done
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-current-contract]
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Concise, expanded, and full agent responses

## Brief

Replace the public response projection matrix with one detail progression: implicit `concise`, explicit `expanded`, and explicit `full`. Concise browser actions expose outcome, page/navigation/focus changes, warnings, anchors, resources, and a bounded flattened exact-target index. Expanded adds bounded semantic/page context; full returns complete acquired structures. Inline image transport remains an orthogonal opt-in.

Delete legacy, compact, interaction-only, omit, and public diagnostic-suppression variants. Failed and degraded results always expose privacy-bounded diagnostics. Update generated schemas, registry routing, skill instructions, documentation, and protocol tests to teach omission-first routine use and deliberate expansion.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: agent-facing contract consumed by batch and temporal economy features

## Simplification opportunity

Delete per-part preference enums and switches, test-only legacy bundles, projection-omitted markers, ancestor-closure reconstruction, duplicate server parsing for diagnostic preferences, and obsolete projection tests. Keep one canonical-result projection path and small MCP-specific concise output types.

## Foundation references

- `docs/VISION.md` — Core Experience
- `docs/SPEC.md` — Current-State Observation and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — MCP Boundary

## Design decisions

- **Public request shape**: keep the existing optional `response` object but replace every per-part selector with `detail: concise | expanded | full` plus `inline_images: bool`. Omission selects `concise`; `inline_images` defaults to `false`. A boolean makes image transport an orthogonal opt-in without retaining another `omit` enum.
- **Target reference shape**: every concise target carries the complete existing `NodeReference`. Repeating the three small identity fields is preferable to asking agents to assemble references from collection-level fields and accidentally mix generations.
- **Bounded omission accounting**: concise and expanded snapshot projections report source node omissions separately from presentation target/context omissions. Presentation counts are exact over the acquired canonical snapshot; Krometrail does not claim whether source-omitted nodes were actionable.
- **Expanded semantics**: expanded output is a larger flattened projection, not another valid `PageSnapshot`. It adds bounded semantic entries with original node/parent/depth hints, while allowing a referenced parent to be absent. This deletes ancestor-closure reconstruction and avoids pretending a pruned tree is canonical.
- **Status integration**: remove the separate `browser_status.detail` request. `browser_status` accepts the common `response` object; concise is the existing operational summary, expanded adds pages while retaining summarized capture/retention, and full serializes the complete current `BrowserStatus`.
- **Failure diagnostics**: failed and degraded structured tool results always receive `ResponseDiagnostics`; JSON-RPC errors already receive the same bounded diagnostic payload. Healthy responses omit diagnostics. There is no request-time diagnostic switch.
- **Canonical-result projection**: all three details acquire and map the same domain result first. Concise/expanded alter only MCP presentation; full emits the complete acquired structure. Inline-image omission never removes canonical resource links, warnings, errors, interaction/range handles, or capture-health degradation.
- **Temporal progression**: concise is the existing small range/artifact/context index; expanded retains complete bounded bundle context while replacing embedded artifact manifests with canonical handles; full serializes the acquired temporal structure. Generic progressive results follow the same direction where they contain large frame/artifact collections; already-small results may be identical across adjacent levels.
- **Dispatch rationale**: direct-read design across the MCP response, schema, server, registry, protocol tests, and plugin skill. The public-schema cross-cut is cohesive in one crate and its exact call sites were locally inspectable; child stories are durable acceptance checkpoints, not parallel ownership bundles.

## Architectural choice

Three approaches were considered:

1. **Keep independent per-part controls and add presets.** A `concise` preset could populate the existing snapshot/page-state/temporal/diagnostic matrix. This minimizes immediate code changes, but retains conflicting combinations, aliases, omission markers, duplicate parsing, and the agent decision burden this feature exists to remove.
2. **Introduce separate concise response DTOs at every operation handler.** Each tool could build its cheapest output directly. This can produce very small responses, but creates parallel acquisition/mapping paths and risks changing warnings, action outcomes, anchors, and resources by detail level.
3. **One response request and one post-acquisition projector.** Canonical results continue through the existing operation/domain mapping, then one typed MCP projector derives concise, expanded, or full presentation. Inline images are filtered independently and diagnostics are attached after final status is known.

Choose option 3. It follows the repository's canonical-result-projection pattern while allowing the old matrix, ancestor closure, omission markers, and diagnostic preference parsing to be deleted outright. It also keeps `crates/krometrail-core` and `crates/krometrail-cdp` authoritative snapshot/reference behavior unchanged.

## Implementation Units

### Unit 1: Collapse the public response wire contract

**Files**:
- `crates/krometrail-mcp/src/response.rs`
- `crates/krometrail-mcp/src/schema.rs`
- `crates/krometrail-mcp/src/registry.rs`
- `crates/krometrail-mcp/src/server.rs`

**Story**: `epic-agent-surface-simplification-response-detail-wire-contract`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseDetail {
    #[default]
    Concise,
    Expanded,
    Full,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseRequest {
    #[serde(default)]
    pub detail: ResponseDetail,
    #[serde(default)]
    pub inline_images: bool,
}

pub(crate) fn split_response_request(
    arguments: JsonObject,
) -> krometrail_core::Result<(JsonObject, ResponseRequest)>;

pub(crate) fn projected_input_schema(
    base: Arc<JsonObject>,
) -> krometrail_core::Result<Arc<JsonObject>>;

fn attach_diagnostics(
    result: &mut CallToolResult,
    correlation_id: &str,
    context: &DiagnosticContext,
) -> &'static str;
```

**Implementation notes**:
- Delete `StructuredResponseDetail`, `SnapshotResponseDetail`, `InlineImageDetail`, `DiagnosticDetail`, `TemporalResponseDetail`, `ResponseProjectionRequest`, `BrowserStatusDetail`, and `BrowserStatusRequest`.
- Keep `response` optional and schema-generated from `ResponseRequest`; accepted examples are `{}`, `{"response":{"detail":"expanded"}}`, `{"response":{"detail":"full","inline_images":true}}`. Reject every removed field/value at the closed schema and serde boundary.
- Project `browser_status` through the common schema and parser. Other lifecycle tools stay on their natural request types and do not advertise a meaningless response knob.
- Rename `*_projected` mapping entry points to their direct current names after deleting the compatibility/test-only wrappers. Registry handlers split `response` once and pass `ResponseRequest` to the mapper.
- Delete `KrometrailMcpServer::projection_routes`, `requested_diagnostic_detail`, and the diagnostic-detail argument. `attach_diagnostics` attaches correlation/log identity whenever the final structured status is `degraded` or `failed`; `attach_error_diagnostics` remains the JSON-RPC error path.

**Acceptance criteria**:
- [ ] Every response-enabled tool schema advertises only `response.detail = concise | expanded | full` and `response.inline_images = boolean`, with both optional and no per-part variants.
- [ ] Omitted `response` and an empty `response` object decode identically to concise/no-inline.
- [ ] Removed fields and values fail as `invalid_input` at the precise `response.<field>` boundary and are not accepted as aliases.
- [ ] Failed and degraded results always include correlation ID and configured log path; succeeded results do not.
- [ ] `browser_status` uses the common response request and no longer accepts top-level `detail`.

---

### Unit 2: Project canonical observations into flattened targets

**File**: `crates/krometrail-mcp/src/response.rs`

**Story**: `epic-agent-surface-simplification-response-detail-projection`

```rust
const MAX_CONCISE_TARGETS: usize = 48;
const MAX_CONCISE_TARGET_JSON_BYTES: usize = 12 * 1024;
const MAX_EXPANDED_CONTEXT_NODES: usize = 96;
const MAX_EXPANDED_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct ExactTarget {
    pub reference: NodeReference,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub states: Vec<AccessibleProperty>,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
pub(crate) struct TargetOmissions {
    pub source_nodes: u32,
    pub presentation_targets: u32,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
pub(crate) struct ExpandedSnapshotOmissions {
    pub source_nodes: u32,
    pub presentation_targets: u32,
    pub presentation_context_nodes: u32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct ExactTargetIndex {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub targets: Vec<ExactTarget>,
    pub omissions: TargetOmissions,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct SemanticContextEntry {
    pub node_id: SnapshotNodeId,
    pub parent_node_id: Option<SnapshotNodeId>,
    pub depth: u16,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub states: Vec<AccessibleProperty>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct ExpandedSnapshot {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub targets: Vec<ExactTarget>,
    pub semantic_context: Vec<SemanticContextEntry>,
    pub omissions: ExpandedSnapshotOmissions,
}

fn concise_snapshot(snapshot: &PageSnapshot) -> Result<ExactTargetIndex, ResponseInvariantError>;
fn expanded_snapshot(snapshot: &PageSnapshot) -> Result<ExpandedSnapshot, ResponseInvariantError>;
fn target_rank(node: &SnapshotNode) -> (u8, usize);
fn project_response(
    tool: &str,
    projection: &mut Projection,
    response: ResponseRequest,
) -> Result<(), ResponseInvariantError>;
```

**Implementation notes**:
- The trickiest unit is target selection. Filter only canonical actionable nodes with complete references, rank focused, editable, other non-link, then link, and use canonical preorder index as the stable tie-break. Admit whole serialized `ExactTarget` entries until both count and byte budgets are reached; never truncate an entry.
- `states` is the acquired accessibility property vector under a clearer presentation name. Do not invent a second state enum or discard unknown present-state properties.
- Expanded starts with the same target index, then admits non-actionable semantic entries under its independent cap/byte budget. Prefer named/value/description-bearing entries and high-signal roles (`alert`, `dialog`, `heading`, `status`) before remaining canonical preorder. Parent/depth fields are context hints; do not include ancestors merely to make a valid tree.
- `source_nodes` copies `PageSnapshot::omitted_node_count`. `presentation_targets` is acquired actionable count minus emitted targets. Expanded alone reports `presentation_context_nodes` as acquired non-actionable count minus emitted semantic entries; concise uses `TargetOmissions` and makes no claim about context it did not request.
- Concise live observations serialize a bounded page identity (`target_id`, URL, title, navigation) and `ExactTargetIndex`; unavailable page/snapshot errors remain warnings. Expanded adds full `PageState` plus `ExpandedSnapshot`. Full serializes the original `LiveObservation`/`PageSnapshot`. Screenshot bytes are included only when `inline_images` is true; metadata/resources already warranted by the specific tool remain available.
- Apply the same response detail consistently to root `snapshot_page`, root `inspect_page`, live observations, post-action observations, and batch final observation. Per-step batch results remain operation summaries and do not acquire a second nested response setting.
- Temporal bundle mapping becomes three explicit stages: full canonical serialization, expanded bundle with compact artifact handles/canonical resources, concise index derived from expanded. Progressive frame/artifact lists gain bounded concise/expanded summaries where size can grow; small mutation/pin results remain identical.
- Browser status projections are `ConciseBrowserStatus`, `ExpandedBrowserStatus`, and full `BrowserStatus`. Expanded adds full page entries while retaining concise capture/retention summaries; it does not reintroduce removed browser compatibility data.
- Delete `SnapshotProjection`, `project_snapshot`, `compact_snapshot`, ancestor-position maps, automatic path byte helpers, `apply_*_part`, `projection_omitted_part`, `compact_page_state`, and test-only legacy mappers. Keep one `project_response` dispatch after canonical operation mapping.

**Acceptance criteria**:
- [ ] Default action and `snapshot_page` responses contain a bounded flat `targets` array, no `nodes` tree, no ancestor closure, and exact copyable `NodeReference` values.
- [ ] Focused and editable targets survive ahead of early links under both count and byte pressure; ordering is deterministic.
- [ ] Concise and expanded outputs distinguish canonical source omissions from exact presentation omissions.
- [ ] Expanded adds bounded semantic/page context without claiming its entries form a complete `PageSnapshot`; full equals the complete acquired structure apart from independently omitted inline bytes.
- [ ] Outcome/status, warnings, error, interaction/range handles, capture degradation, and resource identities are byte-equivalent across detail levels for one canonical result.
- [ ] `inline_images: false` removes only inline MCP image blocks/metadata chosen for inline transport; `true` embeds the same verified bytes without changing structured detail.
- [ ] Projection tests prove serialized count and byte limits on a large Hacker-News-shaped snapshot and prove a late focused textbox is retained.

---

### Unit 3: Replace projection-matrix tests and agent guidance

**Files**:
- `crates/krometrail-mcp/src/response.rs`
- `crates/krometrail-mcp/src/schema.rs`
- `crates/krometrail-mcp/src/server.rs`
- `plugin/skills/krometrail/SKILL.md`
- `plugin/skills/krometrail/references/evidence.md`
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/VISION.md`
- `docs/public/llms-full.txt`

**Story**: `epic-agent-surface-simplification-response-detail-agent-guidance`

```json
{}

{"response":{"detail":"expanded"}}

{"response":{"detail":"full","inline_images":true}}
```

**Implementation notes**:
- Replace matrix-driven tests with one protocol-level progression test that invokes the same mutation under implicit concise, expanded, and full and compares the authoritative envelope fields while asserting only the intended presentation growth.
- Add focused schema tests for the exact two-field response object and server tests proving diagnostics cannot be suppressed on degraded/failed outcomes.
- Delete tests for `legacy`, `compact`, `interaction_only`, per-part `omit`, diagnostic omission, ancestor closure, projection markers, and wrapper behavior whose contract no longer exists. Retain regression tests for action ranking, byte bounds, exact references, inline-image transport, temporal resource links, and capture warnings.
- Rewrite the Krometrail skill's opening guidance around omission-first concise use, `expanded` for broader semantic/page context, and `full` only for complete acquired structures. Teach agents to use targets returned by concise output directly and to opt into pixels with `inline_images: true`.
- Update the evidence reference to the same vocabulary and preserve privacy-bounded diagnostic handling. State that failed/degraded diagnostics are always present, subject only to a genuinely unavailable log path.
- Foundation docs were preflight-rolled forward during epic scope. Verify their assertions against the final shape and replace any drift in place; do not add historical migration prose. Regenerate `docs/public/llms-full.txt` with `bun run docs:build` if a source documentation page changes.

**Acceptance criteria**:
- [ ] Tool schemas, runtime parsing, protocol tests, skill examples, evidence reference, and foundation docs use only concise/expanded/full plus boolean inline image opt-in.
- [ ] The skill makes omitted `response` the preferred routine path and presents expansion as deliberate, not standard.
- [ ] No live source/test/skill reference remains to the removed response variants or diagnostic suppression; historical changelog entries are not rewritten.
- [ ] Obsolete projection tests are deleted rather than translated into compatibility assertions, and the replacement suite protects only current behavior and learned regressions.
- [ ] Generated public documentation is byte-current when source docs change.

## Implementation Order

1. `epic-agent-surface-simplification-response-detail-wire-contract` — establish the only accepted request vocabulary and unconditional diagnostic attachment.
2. `epic-agent-surface-simplification-response-detail-projection` — implement concise/expanded/full presentation and delete the old projection machinery against that contract.
3. `epic-agent-surface-simplification-response-detail-agent-guidance` — replace protocol/schema tests, update the skill/reference, verify foundation assertions, and regenerate derived docs.

## Simplification

- Delete five public detail enums and their combination matrix in favor of one three-variant enum and one boolean.
- Delete the separate browser-status detail contract and route status through the same response vocabulary.
- Delete diagnostic suppression parsing, the server's projected-route registry, and conditional diagnostic branches.
- Delete pruned-`PageSnapshot` reconstruction, ancestor-position/path accounting, omission marker objects, and independent page/snapshot/temporal switch functions.
- Delete compatibility/test wrappers such as `ResponseProjectionRequest::legacy` and `map_*_projected` once their direct current replacements exist.
- Delete obsolete matrix tests instead of preserving every removed combination. Keep only tests protecting current schema closure, target/reference correctness, bounded output, progressive evidence authority, and always-on failure feedback.
- Do not change core snapshot acquisition or reference validation merely to make MCP presentation smaller; those are current correctness authorities, not compatibility cruft.

## Testing

- **Schema interface**: one generated-schema assertion protects the exact closed `response` object and default omission behavior.
- **Protocol interface**: one mutation round trip across concise/expanded/full protects invariant outcome, anchor, warning, resource, and diagnostic semantics while checking presentation growth.
- **Target regression**: a large synthetic snapshot modeled on the observed Hacker News failure mode protects late focused/editable target retention, deterministic ranking, exact references, and both bounds.
- **Failure regression**: server-level failed/degraded/succeeded cases protect unconditional diagnostics only where actionable and removal of the suppression path.
- **Temporal/resource seam**: focused bundle/progressive tests protect full canonical expansion and concise/expanded resource authority without duplicating artifact-generation tests.
- **Test removal**: remove exhaustive cross-products, legacy-wrapper parity, ancestor closure, omission marker, and diagnostic-omit tests because those branches no longer exist.
- **Verification**: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, plus `bun run docs:build` when documentation sources change.

## Risks

- **Reference usability**: flattening can become smaller by factoring out target/generation, but that would make agents reconstruct exact references. Mitigation: preserve complete `NodeReference` per target and protocol-test direct reuse in an action.
- **False omission claims**: source snapshot omissions cannot be classified as actionable or semantic after acquisition. Mitigation: report source omissions separately and compute presentation omissions only over acquired nodes.
- **Full is accidentally not full**: the current temporal bundle starts from compact artifact handles even under `temporal: full`. Mitigation: make canonical serialization the full branch first, then derive expanded and concise; verify complete manifests/source identities only in full.
- **Response growth migrates elsewhere**: flattening snapshots alone could leave progressive frame/artifact lists unbounded under concise. Mitigation: apply the one detail progression to every response-enabled growing collection while leaving already-small results direct.
- **Inline image ambiguity**: a screenshot-specific tool called without `inline_images` returns structured metadata but not pixels. Mitigation: make the schema/skill explicit and test both paths; do not silently override the global opt-in based on tool name.
- **Concurrent current-contract cleanup**: the dependency may remove `BrowserStatus.compatibility` and related tests before this feature lands. Mitigation: design status DTOs against the post-dependency current contract and avoid retaining those fields in projections.

## Implementation result

The MCP surface now has one strict response request: implicit concise, explicit expanded/full, and an
orthogonal boolean inline-image opt-in. Concise snapshots are flat ranked exact-target indexes;
expanded adds bounded semantic context; full preserves canonical acquired structures. Browser status,
live observations, actions, batch final evidence, temporal bundles, and growing progressive evidence use
the same progression. Failed/degraded diagnostics are unconditional.

The implementation deleted the old selector matrix, legacy constructors, status-only request,
diagnostic suppression, projected-route registry, ancestor closure/pruned-tree reconstruction,
omission markers, wrapper entry points, and obsolete combination tests. Agent guidance and visual
evidence instructions now teach the current surface only.

Verification: MCP unit tests (59 passed), workspace compile check, formatting check, and documentation
build completed successfully during implementation.

## Review fixes

- Inline source-frame construction and presentation limits now run only when `inline_images` is true;
  default resource-only reads remain successful even when the same batch would exceed inline limits.
- Expanded snapshot admission now budgets the complete serialized projection, including context,
  generation, omission accounting, keys, and framing, rather than only its two arrays.
- Added focused count-pressure and complete-JSON byte-pressure regressions, and corrected the concise
  temporal inline-image example in the skill.

The single standard fresh-context review otherwise approved the collapsed request schema, target ranking and references, omission accounting, full fidelity, browser status, diagnostics, and current guidance. Its two blockers were fixed in `f2acd5f`; no second review pass was run.
