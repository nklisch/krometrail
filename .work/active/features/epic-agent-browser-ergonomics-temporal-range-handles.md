---
id: epic-agent-browser-ergonomics-temporal-range-handles
kind: feature
stage: implementing
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Temporal resolved-range handles

## Brief

Return an opaque handle alongside resolved temporal ranges and accept that handle anywhere an agent currently has to repeat the full range: artifacts, region filmstrips, source-frame reads, browser events, pin state, and optional video. The process-local authority maps the handle back to the exact validated `ResolvedRange` before existing services run and revalidates retained availability on use.

Handles are immutable conveniences, not persisted evidence identities. They survive browser stop while retained data remains, fail after MCP restart or session deletion, and never replace the full-range contract or canonical provenance.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent temporal-agent ergonomics contract

## Simplification opportunity

Resolve handle-or-range once at the application boundary and keep every store, artifact, browser-event, retention, and video port expressed in exact `ResolvedRange` values.

## Foundation references

- `docs/SPEC.md` — Temporal Ranges and Temporal Queries
- `docs/ARCHITECTURE.md` — Temporal Range Resolution and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — provenance and authoritative source frames

## Design decisions

- **Authority and lifetime**: use one injected process-local immutable handle table. Browser stop does not clear it; MCP restart does. Registration deduplicates equal ranges and never evicts a live entry, so an issued handle does not silently retarget or expire during the process lifetime.
- **Capacity**: bound the table at 4,096 distinct ranges and fail new registration with `resource_limit_exceeded` rather than evicting issued handles. This preserves the lifetime promise and prevents unbounded agent-driven growth.
- **Wire shape**: temporal follow-up tools accept exactly one root property, `range` or `range_handle`. Existing full-range requests and schemas remain valid; no core artifact, event, retention, frame, or video request type learns about handles.
- **Availability**: handle resolution revalidates the exact ordered source-frame metadata against the injected `FrameSource` before dispatch. Missing/session-deleted evidence fails `evidence_invalidated` with recovery to resolve a fresh interval; a stored handle is never evidence authority.
- **Response location**: add optional `range_handle` to the common `ToolResponse` envelope. Bundle responses register their resolved range; follow-up responses echo the supplied or deduplicated handle. Non-temporal and legacy responses omit the field.
- **Identity**: add one typed UUID-backed `ResolvedRangeHandleId` to the existing ID registry and allocate through the injected `IdSource`; handles reveal no range fields and are not persisted.
- **UI surface**: none; this is an MCP and agent-instruction surface, so no mockup is required.

## Architectural choice

Three approaches were considered. Persisting handles beside recordings would create a second retained evidence identity and migration contract. Encoding the full range into a signed token would still create very large inputs and duplicate validation/serialization semantics. The chosen approach is an injected in-memory table at the application boundary: it stores exact validated `ResolvedRange` values, verifies retained source availability on lookup, and hands the unchanged range to existing services. The MCP adapter alone accepts handle-or-range wire input. This keeps storage and evidence ports range-based and makes the convenience explicitly process-local.

The highest-risk unit is lookup revalidation. A handle can outlive the active browser and its frames can later be evicted or deleted; returning the cached range without checking would let event-only or pin-state paths appear usable after their visual evidence disappeared. The authority therefore checks every expected frame ID, scope, order, and retained time before the existing tool executes.

## Implementation Units

### Unit 1: Typed handle contract and process-local authority

**Files**: `crates/krometrail-core/src/ids.rs`, `crates/krometrail-core/src/range_handle.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-core/src/ports/mod.rs`, `src/range_handles.rs`, `src/app.rs`
**Story**: `epic-agent-browser-ergonomics-temporal-range-handles-authority`

```rust
// crates/krometrail-core/src/ids.rs, in the existing typed_ids! registry
ResolvedRangeHandleId,

pub const MAX_RESOLVED_RANGE_HANDLES: usize = 4_096;

pub trait ResolvedRangeHandles: Send + Sync {
    fn register(&self, range: ResolvedRange) -> Result<ResolvedRangeHandleId>;

    fn resolve_available(
        &self,
        handle: ResolvedRangeHandleId,
    ) -> PortFuture<'_, Result<ResolvedRange>>;

    fn invalidate_session(&self, session_id: SessionId) -> Result<usize>;
}

pub struct ProcessResolvedRangeHandles {
    ids: Arc<dyn IdSource>,
    frames: Arc<dyn FrameSource>,
    entries: Mutex<HashMap<ResolvedRangeHandleId, ResolvedRange>>,
}

impl ProcessResolvedRangeHandles {
    pub fn new(ids: Arc<dyn IdSource>, frames: Arc<dyn FrameSource>) -> Self;
}
```

**Implementation notes**:

- `register` calls `ResolvedRange::validate`, returns the existing handle for an exactly equal range, rejects nil/colliding IDs as internal contract failures, and refuses the 4,097th distinct range without removing prior entries.
- `resolve_available` clones the range while holding the synchronous mutex, releases the lock, then calls `FrameSource::frame_metadata_by_id(range.frame_ids.clone())`. It requires the exact count and order plus matching session, target, frame ID, and a session time inside the resolved range. It never reads frame pixels.
- Unknown handles—including valid UUIDs from a previous process—return `evidence_invalidated`, retry `after_recovery`, with recovery instructing the caller to run `temporal_debug_bundle` again. Invalid wire UUIDs remain `invalid_input`.
- `invalidate_session` removes only entries for the exact session. It is invoked by any composed public session-deletion path; current browser stop intentionally does not invoke it. Downstream availability validation remains mandatory because budget eviction can occur without an explicit deletion callback.
- Construct one authority in `app.rs` from the same root `IdSource` and coherent recording store used by temporal services, then inject it into `McpDependencies`.

**Acceptance criteria**:

- [ ] Equal resolved ranges receive the same handle within one process; different ranges never share a handle.
- [ ] Handles remain resolvable after browser stop while every expected frame remains retained.
- [ ] Unknown/restarted, invalidated-session, partially evicted, reordered, or cross-scope evidence fails before a temporal follow-up service is called.
- [ ] The capacity boundary rejects only new distinct ranges and leaves every previously issued handle usable.

### Unit 2: Additive handle-or-range schema and normalization

**Files**: `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/config.rs`
**Story**: `epic-agent-browser-ergonomics-temporal-range-handles-followups`

```rust
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedRangeHandleArgument {
    pub range_handle: ResolvedRangeHandleId,
}

pub(crate) fn range_handle_input_schema(
    base: Arc<JsonObject>,
) -> Result<Arc<JsonObject>>;

async fn resolve_range_argument(
    mut arguments: JsonObject,
    handles: &dyn ResolvedRangeHandles,
) -> Result<(JsonObject, ResolvedRangeHandleId)>;
```

**Implementation notes**:

- `range_handle_input_schema` recognizes a root generated object with a `range` property, adds `range_handle`, removes `range` from the unconditional required list, and adds an exclusive `oneOf` requiring exactly one. It leaves non-range retrieval routes unchanged and fails registry validation if a declared range-follow-up route cannot be transformed.
- Runtime normalization rejects both fields together. With `range_handle`, it resolves availability and inserts the serialized exact range before the existing typed decoder. With `range`, it first decodes through the existing request type, then registers that exact validated range for response echo.
- Apply the transform to `generate_artifacts`, `generate_region_filmstrip`, `list_source_frames`, `fetch_source_frames`, `pin_resolved_range`, `query_pin_state`, `unpin_resolved_range`, `query_browser_events`, and conditional `generate_temporal_video`. Artifact/source-frame resource retrieval routes that already use exact resource IDs do not gain range handles.
- `McpDependencies` receives `Arc<dyn ResolvedRangeHandles>`; no concrete table leaks into the MCP crate.

**Acceptance criteria**:

- [ ] Every named follow-up schema accepts legacy full `range` or `range_handle`, requires exactly one, remains closed, and continues to advertise its existing non-range fields and limits.
- [ ] The normalized request reaches the existing core deserializer/service as the exact registered `ResolvedRange`; no downstream port signature changes.
- [ ] A handle lookup error prevents artifact, frame, event, pin, and video service dispatch and returns the stable sanitized error envelope.

### Unit 3: Handle publication, echo, and projection compatibility

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/server.rs`
**Story**: `epic-agent-browser-ergonomics-temporal-range-handles-followups`

```rust
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ToolResponse {
    // existing fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_handle: Option<ResolvedRangeHandleId>,
}

impl MappedResult {
    pub(crate) fn with_range_handle(
        mut self,
        handle: ResolvedRangeHandleId,
    ) -> Self;
}
```

**Implementation notes**:

- After a successful bundle service result, register `bundle.range` before mapping and attach the handle to the response. Registration failure returns a normal stable tool error; it does not discard or mutate retained evidence.
- Follow-up routes carry the normalized handle into mapping and echo it on success or degradation. A full-range follow-up registers/deduplicates the decoded range and returns the resulting handle.
- The response-projection feature must never omit `range_handle`; it is a drill-down identity alongside warnings, interaction IDs, and canonical resource URIs.
- Do not insert the handle into artifact manifests, video manifests, resource URIs, `ResolvedRange`, or persisted rows.

**Acceptance criteria**:

- [ ] `temporal_debug_bundle` returns one handle beside the unchanged full range and the handle can drive each named follow-up without copying range JSON.
- [ ] Legacy requests/responses retain their existing `range` fields; the optional common envelope field is absent on non-temporal operations.
- [ ] Compact response projection preserves the handle and all canonical resource/provenance links.

### Unit 4: Agent guidance and end-to-end contract tests

**Files**: `crates/krometrail-mcp/src/server.rs`, `crates/krometrail-mcp/src/test_fixture.rs`, `src/app.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`
**Story**: `epic-agent-browser-ergonomics-temporal-range-handles-followups`

**Implementation notes**:

- Update the skill to retain the `range_handle` from the bundle and use it for focused tools; copy the full `range` only when crossing MCP process boundaries or preserving an exact external record.
- Explain that handles are local conveniences, survive browser stop, do not survive plugin/MCP restart, and do not weaken gap, retention, or provenance checks.
- Add stdio tests using one real router/dependency assembly: bundle → list frames → events → pin state and, when enabled, video schema/dispatch. Use spies to prove exact range forwarding and zero dispatch after invalidation.
- Regenerate canonical tool schemas through the existing generation path.

**Acceptance criteria**:

- [ ] One end-to-end test proves bundle-to-follow-up reuse and one proves restart/unknown-handle recovery without a browser.
- [ ] Existing full-range stdio and generated-schema tests remain green unchanged.
- [ ] Plugin guidance never describes a handle as persisted evidence or a replacement for manifest provenance.

## Implementation order

1. `epic-agent-browser-ergonomics-temporal-range-handles-authority`: typed ID, bounded authority, availability validation, composition wiring, and authority tests.
2. `epic-agent-browser-ergonomics-temporal-range-handles-followups`: schema normalization, all follow-up routes, response publication, stdio coverage, and skill guidance.

## Simplification

- Keep `ResolvedRange` as the only input to core artifact, event, frame, retention, and video services; one normalization helper replaces range JSON before existing decoding.
- Use the existing typed-ID macro, `IdSource`, `FrameSource`, `ToolResponse`, route registry, and schema dereferencer. Do not create a persisted handle table, range-token codec, or parallel temporal operation registry.
- Centralize retained-availability verification in the handle authority rather than adding tool-specific handle checks.
- Preserve existing full-range tests as compatibility coverage and add table-driven schema assertions instead of duplicating one test per route.

## Testing

- Authority unit tests protect deduplication, collision/capacity behavior, process lifetime, session invalidation, and exact metadata revalidation.
- Schema table tests protect exclusive handle-or-range input across every named follow-up and unchanged legacy limits.
- Router/stdio tests protect bundle publication, exact normalized forwarding, response echo, invalidation recovery, and conditional video registration.
- Existing store/service tests continue to protect range validity, eviction, gaps, pin semantics, artifact provenance, and video bounds; the feature does not duplicate them.

## Risks

- The key risk is promising process-lifetime handles while bounding memory. Non-evicting deduplication plus an explicit 4,096-range admission failure preserves already-issued handles; the fallback is to raise the documented cap only with measured process-memory evidence.
- Revalidating every frame's metadata adds a storage read before follow-ups. This is proportional to already-bounded resolved ranges and avoids a correctness hole; if profiling later shows material cost, an invalidation generation may optimize it without changing the public handle contract.
- Schema decoration and runtime normalization could diverge. Both are exercised from the registry over the same detected root `range` shape, and all existing core decoders remain the final validation authority.
- Concurrent eviction after preflight remains possible. Existing downstream services must keep their own availability checks; preflight narrows the race but does not replace transactional store semantics.
