---
id: epic-temporal-debugging-workflow-mcp-investigation-surface
kind: feature
stage: done
tags: [agent-ux, visual, browser, storage]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Temporal Investigation Surface

## Brief

Expose the completed temporal workflow through MCP: one debug-bundle tool as the primary range or interaction entry point, plus focused tools for source frames, region filmstrips, artifact variants, verbose browser-event detail, and pin state. Derive names, capability membership, schemas, routing, and annotations from the existing capability/operation contract patterns so disabling temporal vision removes its tools without affecting ordinary browser control.

Present compact summaries and a context-sized primary image through the established response envelope, while full-resolution artifacts and source frames are readable through durable MCP resources tied to retained evidence. Resource reads enforce the same session/target, provenance, retention, and stable-error rules as focused tools; eviction or deletion becomes an honest unavailable-resource result rather than a stale file leak.

Qualify the integrated local agent workflow from browser interaction anchor through resolved range, bundle generation/cache, correlated warnings/events, region or source drill-down, pinning, and resource retrieval using deterministic local scenarios. This is product capability qualification, not paid multimodal evaluation; remote transports, automatic diagnosis, replay, cross-session comparison, page/framework state, and the separate thesis benchmark remain out of scope.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: final integration capability — presents the primary bundle and progressive evidence after both domain paths are complete
- Completed dependencies: `epic-temporal-debugging-workflow-temporal-debug-bundle`, `epic-temporal-debugging-workflow-progressive-evidence-and-pinning`

## Simplification opportunity

- Extend the existing dynamic MCP router, response mapper, stdio service, and one capability registry instead of creating a temporal-only server or handwritten schema mirror.
- Reuse the existing `TemporalDebugBundles`, `ProgressiveEvidence`, `TemporalContextQuery`, `RecordingStore`, artifact cache, frame reader, retention authority, and `ToolResponse` envelope. MCP adds presentation and protocol translation, not another temporal service, cache, payload map, or storage path.
- Add one scoped source-frame read operation for resource reads because a resource URI has an ID and scope but no resolved range. Do not synthesize a fake range or expose a raw store method.

## Grounding and design decisions

- **Prepublic contract:** This feature is a clean first public temporal MCP surface. It adds no compatibility aliases, legacy URI forms, migrations, silent fallback routes, or shims. The exact contracts below are the only supported forms.
- **SDK/protocol:** Use exactly `rmcp = "=0.11.0"` and advertise/negotiate `ProtocolVersion::V_2025_06_18`. Use only tools, resources/templates, stdio, structured tool content, resource links, blob resource reads, and request cancellation already present in that SDK. Do not advertise subscriptions, list-change notifications, task metadata, task polling, or any later MCP capability.
- **One root dependency bundle:** Change the prepublic `krometrail_mcp::build_service` boundary to receive one `McpDependencies` value containing `BrowserConnector`, `TemporalDebugBundles`, `ProgressiveEvidence`, and `TemporalContextQuery`. The root passes the already-created services; MCP never imports CDP, SQLite, segment storage, or temporal-vision.
- **Capability ownership:** `temporal_debug_bundle`, source/artifact/region/pin operations, and evidence resources belong to `CapabilityId::TemporalVision`. `query_browser_events` belongs to `CapabilityId::BrowserEvents`. Browser-control registration is unchanged and remains controlled only by `CapabilityId::Control`. A bundle remains callable when browser-event presentation is disabled; its nested context reports the exact unavailable/empty evidence rather than making the bundle depend on the event tool.
- **Route metadata:** Keep `PROGRESSIVE_EVIDENCE_REGISTRY` as the single source for its operation names, request/result associations, descriptions, capability membership, mutability, and exposure. Add a small macro-backed `TEMPORAL_CONTEXT_OPERATION_REGISTRY` for the verbose event operation and a `TEMPORAL_DEBUG_BUNDLE_OPERATION` definition for the primary operation. The MCP route builder consumes these definitions; it does not repeat stable names or schemas. Resource-only read operations remain in the progressive registry but are not registered as tools.
- **Tool topology:** Register exactly these temporal tools when their capability is enabled:
  - `temporal_debug_bundle` — natural range/interaction entry point; `TemporalDebugBundleRequest`.
  - `list_source_frames` — bounded metadata and durable links for a resolved range; `SourceFramesRequest` with `ResolvedOrder`.
  - `fetch_source_frames` — bounded selected source-frame images and durable links; `SourceFramesRequest` with explicit IDs and MCP inline limits.
  - `generate_artifacts` — supported artifact variants over an already resolved range; `GenerateArtifactsRequest`.
  - `generate_region_filmstrip` — fixed region filmstrip over an already resolved range; `RegionFilmstripEvidenceRequest`.
  - `pin_resolved_range`, `unpin_resolved_range`, `query_pin_state` — the existing resolved-range retention operations.
  - `query_browser_events` — chronological verbose event detail for an already resolved range; `BrowserEventDetailRequest`.
  `retrieve_artifact` and the new `retrieve_source_frame` remain service operations used by resource reads, not duplicate tools that merely wrap `resources/read`.
- **Resolved-range discipline:** The primary bundle is the only MCP temporal tool that accepts a natural anchor. Every other temporal tool consumes the exact `ResolvedRange` returned by the bundle or a prior query. MCP does not parse times, calculate windows, resolve current references, read timeline rows, or call a second range resolver.
- **Resource lifetime:** A resource URI is a weak scoped evidence handle, not a lease. Every read revalidates session/target, retained row, source links, manifest/hash/length, and deletion state through `ProgressiveEvidence`. Eviction or deletion returns a protocol resource-not-found result whose data contains the stable Krometrail error and recovery guidance. No path, segment address, SQLite key, or user-provided URI is accepted.
- **Inline image policy:** The bundle response includes at most one primary persisted artifact image, selected deterministically from available `BeforeDuringAfter`, then `Storyboard`, then `DifferenceMap` outcomes. MCP reads the selected artifact through the same progressive read authority and sends its unchanged PNG bytes as image content only when it fits `MCP_INLINE_IMAGE_MAX_BYTES` (8 MiB). Otherwise it returns the compact structured bundle and resource link with `Degraded` status and an explicit inline-limit warning; MCP never resizes, decodes, re-encodes, or invents a preview. Full artifact bytes remain available through the resource URI.
- **Source-frame image policy:** `fetch_source_frames` permits at most four requested frames, 4 MiB per frame, and 16 MiB total at the MCP presentation boundary. It may return those unchanged JPEG/PNG bytes as image content. `list_source_frames`, generated artifacts, region generation, bundle results, and pin/event operations return metadata and resource links only. Resource reads retain the larger domain limits.
- **Response envelope:** Extend the existing `ToolResponse` once with `resources: Vec<ResponseResource>`, and generalize `ResponseImage.metadata` to a tagged screenshot-or-artifact metadata value. Existing control tools continue to use the same envelope and direct screenshot image blocks. Temporal structured results contain no encoded bytes or base64; resource links appear both in the stable envelope metadata and as `ResourceLink` content blocks.
- **No UI:** This is an API/resource surface with no human screen or journey. Mockups are intentionally skipped.
- **Review/dispatch:** Direct local design only. The user explicitly prohibited implementation, peer review, and push. The public contracts and local SDK source were sufficient; no external research or advisory pass is required for this design.

## Exact MCP contracts

### Tool request and result boundary

All temporal tool routes use the established output schema generated from `ToolResponse`. The route exposes the inner domain request object, not a tagged operation wrapper. The handler injects any registry tag internally before deserializing the exact domain request. A malformed request reaches no application service.

```rust
// crates/krometrail-mcp/src/config.rs
#[derive(Clone)]
pub struct McpDependencies {
    pub browser: Arc<dyn BrowserConnector>,
    pub temporal_debug_bundles: Arc<dyn TemporalDebugBundles>,
    pub progressive_evidence: Arc<dyn ProgressiveEvidence>,
    pub temporal_context: Arc<dyn TemporalContextQuery>,
}

// crates/krometrail-mcp/src/server.rs
pub fn build_service(
    dependencies: McpDependencies,
    config: McpConfig,
) -> Result<McpService>;
```

The MCP adapter creates one `McpCancellation(CancellationToken)` per request. Temporal calls receive a common absolute deadline, with a 30-second MCP presentation ceiling. The bundle service retains its stricter internal 20-second policy. The progressive service receives `ProgressiveEvidenceContext { deadline, cancellation, current_reference_geometry }`; the current-geometry value is an `Arc<dyn CurrentReferenceGeometry>` view of the same `BrowserSessionOwner`. Direct context reads use the same cancellation/deadline in an adapter `select!`; dropping a read future is safe because context queries are read-only.

```rust
// crates/krometrail-core/src/progressive.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetrieveSourceFrameRequest {
    pub scope: EvidenceScope,
    pub frame_id: FrameId,
    max_encoded_bytes: NonZeroU64,
}

impl RetrieveSourceFrameRequest {
    pub fn new(
        scope: EvidenceScope,
        frame_id: FrameId,
        max_encoded_bytes: u64,
    ) -> Result<Self>;
    pub const fn max_encoded_bytes(&self) -> u64;
}
```

`RetrieveSourceFrameRequest` rejects nil scope/frame IDs, zero or over-ceiling limits, and unknown fields. `FrameSource` adds `read_source_frame(RetrieveSourceFrameRequest) -> PortFuture<'_, Result<SourceFrameRead>>`; `RecordingStore` implements it with the existing optimistic snapshot/read/revalidate protocol. The progressive operation registry gains `RetrieveSourceFrame(RetrieveSourceFrameRequest) => SourceFrameRead { stable_name: "retrieve_source_frame", exposure: ResourceOnly }`. It is dispatched by the existing `ProgressiveEvidenceService` and is never a public tool. Each definition has this generated metadata shape:

```rust
pub struct ProgressiveEvidenceOperationDefinition {
    pub kind: ProgressiveEvidenceOperationKind,
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
    pub exposure: OperationExposure, // Tool | ResourceOnly
    pub request_type: &'static str,
    pub result_type: &'static str,
}
```

```rust
// crates/krometrail-core/src/timeline/context.rs
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BrowserEventDetailRequest(TemporalContextRequest);

impl BrowserEventDetailRequest {
    pub fn new(
        range: ResolvedRange,
        clip: Option<SessionRange>,
        filter: BrowserEventFilter,
        limit: u16,
        cursor: Option<BrowserEventCursor>,
        focus_times: Vec<SessionTime>,
    ) -> Result<Self>;
    pub fn into_context_request(self) -> TemporalContextRequest;
    pub const fn context_request(&self) -> &TemporalContextRequest;
}
```

The wrapper serializes the existing exact context wire shape (`range`, `clip`, `filter`, `selection`, `focus_times`) but always constructs `BrowserEventSelection::Chronological`. It rejects compact selection, zero/over-ceiling page limits, cursor/filter/range mismatches, out-of-range focus times, and unknown fields before I/O. Its operation definition is:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporalContextOperationKind {
    QueryBrowserEvents,
}

pub struct TemporalContextOperationDefinition {
    pub kind: TemporalContextOperationKind,
    pub stable_name: &'static str, // "query_browser_events"
    pub description: &'static str,
    pub capability: CapabilityId,  // BrowserEvents
    pub mutability: OperationMutability, // ReadOnly
}

pub static TEMPORAL_CONTEXT_OPERATION_REGISTRY: &[TemporalContextOperationDefinition] = &[
    TemporalContextOperationDefinition {
        kind: TemporalContextOperationKind::QueryBrowserEvents,
        stable_name: "query_browser_events",
        description: "Read chronological browser-event detail for a resolved temporal range.",
        capability: CapabilityId::BrowserEvents,
        mutability: OperationMutability::ReadOnly,
    },
];

pub struct TemporalDebugBundleOperationDefinition {
    pub stable_name: &'static str, // "temporal_debug_bundle"
    pub description: &'static str,
    pub capability: CapabilityId,  // TemporalVision
    pub mutability: OperationMutability, // ReadOnly
}

pub const TEMPORAL_DEBUG_BUNDLE_OPERATION: TemporalDebugBundleOperationDefinition =
    TemporalDebugBundleOperationDefinition {
        stable_name: "temporal_debug_bundle",
        description: "Inspect a resolved interaction or temporal range as a compact evidence bundle.",
        capability: CapabilityId::TemporalVision,
        mutability: OperationMutability::ReadOnly,
    };
```

`TemporalDebugBundleRequest` remains the exact existing natural-anchor contract. Its generated MCP schema is derived from its validated private wire shape, not handwritten JSON. `TemporalDebugBundle` remains the authoritative result; MCP adds no bundle DTO or URI field.

### Tool registry and annotations

The MCP registry loops over operation definitions and produces `ToolRoute::new_dyn` routes. Each route gets:

- the definition's stable name and description;
- a schema generated from the exact request wire type with rmcp 0.11's draft-2020-12 schema settings;
- the common `ToolResponse` output schema;
- annotations derived from definition metadata.

Annotations use the existing conservative mapping: read-only operations set `readOnlyHint=true`, `idempotentHint=true`, `destructiveHint=false`; state-changing retention operations set `readOnlyHint=false`, `idempotentHint=true`, and `destructiveHint=true` for unpin because budget enforcement can release evidence. All temporal tools set `openWorldHint=true` because they inspect local browser/session evidence. Artifact generation is read-only at the browser/evidence meaning even though it may populate a derived cache.

The route builder fails closed if an operation definition has an empty name/description, a non-object schema, duplicate names, a disabled capability mismatch, or a missing registry/request association. It sorts tools by stable name before `tools/list`, matching the existing rmcp 0.11 router behavior.

### Durable resource URI grammar

The MCP adapter advertises two resource templates and no concrete `resources/list` entries:

```text
krometrail://evidence/{session_uuid}/{target_uuid}/artifacts/{artifact_uuid}
krometrail://evidence/{session_uuid}/{target_uuid}/frames/{frame_uuid}
```

The canonical parser accepts only:

- scheme `krometrail`;
- authority `evidence`;
- exactly four path segments: canonical lowercase non-nil session UUID, canonical lowercase non-nil target UUID, literal `artifacts` or `frames`, and canonical lowercase non-nil typed UUID;
- no userinfo, port, query, fragment, empty segment, percent encoding, path traversal, alternate scheme, or extra segment.

The artifact template has fixed `image/png` metadata. The frame template omits template MIME because source JPEG/PNG format is data-dependent; each successful read returns its exact MIME. Resource names and descriptions contain only the typed resource kind and UUID, never labels, URLs, page text, paths, or manifest parameters.

```rust
// rmcp 0.11 resource read projection
ReadResourceResult {
    contents: vec![ResourceContents::BlobResourceContents {
        uri: canonical_uri,
        mime_type: Some(exact_media_type),
        blob: STANDARD.encode(request_scoped_bytes),
        meta: None,
    }],
}
```

`resources/read` parses the URI, creates `RetrieveArtifactRequest` or `RetrieveSourceFrameRequest` with the URI scope and the resource read ceiling, and invokes `ProgressiveEvidence` exactly once. It never opens a file or calls a store port. The returned blob is request-scoped and discarded after the protocol response. `resources/subscribe` and `resources/unsubscribe` remain unsupported and are not advertised; resource invalidation is observed by re-read, not a future subscription mechanism.

### Response/resource projection

```rust
// crates/krometrail-mcp/src/response.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRole { Artifact, SourceFrame }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct ResponseResource {
    pub role: ResourceRole,
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub encoded_byte_len: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseImageMetadata {
    Screenshot(ScreenshotMetadata),
    Artifact {
        artifact_id: ArtifactId,
        media_type: String,
        encoded_byte_len: u64,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct ResponseImage {
    pub role: ImageRole,
    pub step_index: Option<u32>,
    pub metadata: ResponseImageMetadata,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ToolResponse {
    pub tool: String,
    pub status: ToolResponseStatus,
    pub result: Value,
    pub interaction: Option<InteractionAnchor>,
    pub warnings: Vec<KrometrailError>,
    pub images: Vec<ResponseImage>,
    pub resources: Vec<ResponseResource>,
    pub error: Option<KrometrailError>,
}
```

The projector is exhaustive over the temporal result families but not over domain algorithms. It serializes handles, manifests, ranges, context, pin state, and exact nested warnings; it strips only request-scoped bytes from structured JSON. For each `ArtifactHandle`, `ArtifactEvidenceHandle`, or `SourceFrameHandle`, it creates one canonical resource descriptor and one `Content::resource_link(RawResource)` with matching URI/name/MIME/size. It never creates links from arbitrary strings.

The bundle projector chooses the primary artifact by `(preferred_kind, epoch_index, generator_index, artifact_id)`, where preferred kinds are `BeforeDuringAfter`, `Storyboard`, and `DifferenceMap`. It calls the resource authority with the same cancellation/deadline to obtain unchanged bytes for the inline image. A successful inline image adds `ImageRole::TemporalPrimary`, `ResponseImageMetadata::Artifact`, and one `Content::image`; a size/lifetime failure leaves the resource link, adds a stable degradation warning, and does not fail an otherwise usable bundle. No full frame collection or event body is embedded automatically.

The fetch-source-frames projector emits at most four direct image blocks in request order and emits resource links for every returned handle. It does not place base64 in `ToolResponse.result`. If the application result contains a source payload whose bytes disagree with its handle, the adapter returns a protocol-level invariant error rather than publishing a misleading link or image.

Tool errors preserve the existing visible structured failure convention: malformed arguments and domain errors return `Ok(CallToolResult::error(...))`, with a bounded text summary and `ToolResponse.error`. `NotFound`, `EvidenceInvalidated`, cancellation, retention gaps, and resource-limit conditions retain their stable code, context, retry advice, and recovery. Adapter mapping invariants and serialization failures are rmcp internal errors and are logged without payloads.

Resource errors use the narrowest rmcp protocol category while retaining the domain error in `data.krometrail_error`:

- malformed URI or unsupported resource kind → `invalid_params`;
- evicted, deleted, missing, or invalidated evidence → `resource_not_found`;
- cancellation/deadline → `internal_error` with stable `cancelled` data;
- persistence or adapter failure → `internal_error` with stable error data.

No resource error returns bytes, a filesystem path, or a stale success-shaped blob.

## Architectural options

### Option A — MCP handlers read the store and assemble temporal results

Each route could resolve ranges, call artifact/context/progressive services, read SQLite/segments, and build its own links. This violates the MCP boundary, duplicates policy and lifetime checks, leaks storage concerns into protocol code, and makes cache/resource behavior diverge. Rejected.

### Option B — add a temporal-only MCP server and a second evidence protocol

A separate server could expose temporal tools and resources without changing the current control server. It would duplicate stdio/session lifecycle, capability filtering, cancellation, response envelopes, and protocol negotiation. It would also make one process advertise conflicting tool/resource surfaces. Rejected.

### Option C — extend the existing dynamic adapter over injected application ports (chosen)

Core owns validated temporal requests, operation registries, and source/resource read semantics. MCP owns route/schema/annotation generation, URI parsing, resource content, bounded inline image projection, and rmcp error translation. The root injects one dependency bundle and one session owner supplies current geometry. This keeps one server, one protocol version, one response envelope, and one source/artifact/retention authority.

## Architectural choice

Choose Option C. The adapter is a thin presentation boundary with two deliberate protocol-only responsibilities: converting stable evidence handles into validated resource links, and optionally loading one bounded persisted image for immediate model inspection. It never performs image transformation or evidence selection. Resource reads route through the already-tested progressive service so artifact provenance, source retention, session deletion, cancellation, and stable errors cannot fork between tools and resources.

The trickiest unit is weak resource lifetime across the gap between tool response and `resources/read`. A successful bundle or frame listing can be evicted immediately after return, and a URI must never turn that weak handle into a stale file read. The strict URI grammar, scoped core read request, optimistic store validation, and protocol-level unavailable result form one authority chain:

```text
tool result / resource link
        │ typed UUID scope only
        ▼
canonical URI parser
        │
        ▼
ProgressiveEvidence::execute(retrieve_* scoped request)
        │
        ├── validate row, provenance, retention, hash, length
        ├── read bounded bytes outside the mutation gate
        ├── revalidate exact lifetime before return
        └── return blob or stable unavailable-resource error
```

## Implementation units

### Unit 1: temporal MCP contracts, registries, schemas, and scoped source reads

**Story:** `epic-temporal-debugging-workflow-mcp-investigation-surface-contracts-registries-and-resource-read`

**Files:**

- `crates/krometrail-core/src/progressive.rs`
- `crates/krometrail-core/src/ports/frames.rs`
- `crates/krometrail-core/src/timeline/context.rs`
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/krometrail-mcp/src/{config.rs,schema.rs}`
- `crates/krometrail-store/src/{recording.rs,index/frames.rs}`
- focused core/store contract tests

Add the exact `RetrieveSourceFrameRequest`, the `FrameSource::read_source_frame` port, and the progressive resource-only operation/result association. Extend the existing progressive definitions with description, `CapabilityId`, mutability, and `Tool`/`ResourceOnly` exposure metadata. Add the validated chronological browser-event wrapper and one context-operation registry. Add the debug-bundle operation descriptor. Generate Schemars schemas from the exact validated wire types, including custom delegates for private fields and non-schema payload-bearing result types. No domain type gains an MCP URI or resource link.

**Acceptance criteria:**

- [ ] The nine progressive operations have unique stable names, exact request/result associations, descriptions, capability/mutability metadata, and explicit resource-only exposure for both scoped read operations.
- [ ] Scoped source-frame read validation rejects nil/wrong scope, missing IDs, zero/over-limit bytes, and unknown fields before storage I/O; its result uses the existing `SourceFrameRead` handle/hash/lifetime contract.
- [ ] Chronological event requests cannot advertise or deserialize compact selection; range, cursor, focus, filter, and page limits are revalidated at the boundary.
- [ ] Bundle, event, and tool-exposed progressive request schemas are object-root schemas generated from their validated wire contracts; no handwritten JSON schema or MCP DTO mirror exists.
- [ ] Store tests prove scoped source reads use the same optimistic two-gate validation as progressive fetches and return no stale bytes across eviction/deletion.
- [ ] Existing domain artifact, progressive, browser-control, and temporal context contracts remain otherwise unchanged; no compatibility path or schema migration is introduced.

### Unit 2: injected MCP dependencies, session geometry, and capability-driven temporal routes

**Story:** `epic-temporal-debugging-workflow-mcp-investigation-surface-routing-session-and-cancellation`

**Files:**

- `crates/krometrail-mcp/src/{config.rs,session.rs,registry.rs,schema.rs,server.rs}`
- `crates/krometrail-mcp/src/lib.rs`
- `crates/krometrail-core/src/ports/browser.rs` only for a necessary current-geometry adapter correction
- focused MCP route/session tests

Replace the prepublic browser-only `build_service` input with `McpDependencies`. Make `BrowserSessionOwner` implement the existing `CurrentReferenceGeometry` view by delegating to its active `BrowserSessionPort`; it exposes no new browser operation or session identity. Build temporal routes from operation definitions and enabled capabilities. The bundle route calls `TemporalDebugBundles::bundle` once. Progressive routes construct the matching `ProgressiveEvidenceRequest` and call `ProgressiveEvidence::execute` once. The browser-event route converts `BrowserEventDetailRequest` to one `TemporalContextQuery::context` call under the shared cancellation/deadline. Current-reference region requests receive the session-owner geometry view; historical-only requests do not require an active browser.

**Acceptance criteria:**

- [ ] Default configuration lists the four lifecycle tools, 24 control tools, and exactly the enabled temporal tool names once; disabling temporal vision removes temporal tools/templates without removing control tools, and disabling browser events removes only `query_browser_events`.
- [ ] Tool schemas, descriptions, capability membership, stable names, and annotations are derived from the registries; route startup fails closed on duplicate/missing/invalid definitions.
- [ ] A valid primary, progressive, event, and current-reference request reaches exactly one intended application port with the exact typed request and one request-scoped cancellation signal; malformed input reaches none.
- [ ] Current-reference calls delegate through the active session owner and return the existing lifecycle/stale-reference errors; no MCP/CDP type enters core and no live session is required for retained historical reads.
- [ ] Cancellation before dispatch and deadline expiry before/while a route call return stable `Cancelled` without publishing a partial tool result; one request does not cancel another or stop the browser session.
- [ ] rmcp remains exactly 0.11.0 and no task, subscription, HTTP, remote transport, or future protocol capability is referenced.

### Unit 3: stable temporal response mapping, resource URI authority, and bounded inline images

**Story:** `epic-temporal-debugging-workflow-mcp-investigation-surface-response-resources-and-inline-evidence`

**Files:**

- `crates/krometrail-mcp/src/{response.rs,resources.rs,registry.rs}` (resources module new)
- `crates/krometrail-mcp/src/schema.rs`
- focused response/resource tests

Extend `ToolResponse` with resource metadata and generalize image metadata to screenshot/artifact variants. Implement one canonical URI builder/parser and projection helpers for artifact/source handles. Map all binary-bearing domain results to metadata plus resource links; only the bounded primary artifact and bounded selected source frames may become unchanged MCP image content. Implement `resources/read` conversion through the progressive service's resource-only operations, with exact MIME, base64 only in `BlobResourceContents`, stable resource-not-found mapping, and no raw path access.

**Acceptance criteria:**

- [ ] Bundle results return compact structured JSON, exact nested provenance/warnings/degradations, deterministic resource links, and at most one inline primary image; no encoded bytes/base64/data URL/path/segment address appears in structured content or logs.
- [ ] Artifact/source-frame links use only canonical typed UUID scope and ID, match the resource metadata and `Content::resource_link`, and reject every non-canonical/alternate URI form.
- [ ] `resources/read` returns one exact blob with the stored MIME and original bytes for a retained artifact/frame, and eviction, source loss, artifact invalidation, session deletion, wrong scope, and hash/length disagreement never return success bytes.
- [ ] Bundle primary-image selection is stable across outcome order/cache hits and uses only `BeforeDuringAfter`, `Storyboard`, or `DifferenceMap`; over-limit/lifetime failure degrades without losing the durable link.
- [ ] Fetch-source-frames emits at most four direct images under 4 MiB each/16 MiB total, preserves request order, and always keeps the full-resolution resource path available.
- [ ] Existing control response tests continue to prove screenshot bytes are image content only, while temporal response tests prove no full result JSON is duplicated into text.

### Unit 4: rmcp resource capability, stdio/server lifecycle, and root composition

**Story:** `epic-temporal-debugging-workflow-mcp-investigation-surface-resource-server-and-root-composition`

**Files:**

- `crates/krometrail-mcp/src/{server.rs,resources.rs}`
- `src/app.rs`
- `crates/krometrail-mcp/src/{lib.rs,session.rs}` only for public wiring corrections
- in-memory protocol tests and root composition tests

Override rmcp 0.11 `ServerHandler::list_resource_templates`, `list_resources`, and `read_resource`. Advertise `ServerCapabilities::enable_tools().enable_resources().build()` and protocol `V_2025_06_18`; do not advertise subscriptions or list-change support. Return empty concrete-resource lists because retained evidence is potentially large and dynamic; return the two strict templates. Wire `McpDependencies` from the already-composed root `RuntimeDependencies`, preserving one `RecordingStore`, one artifact service/cache, one temporal bundle service, one progressive service, and one browser session owner. Keep stdout exclusively protocol frames and all logs on stderr.

**Acceptance criteria:**

- [ ] Real rmcp initialize negotiates `2025-06-18`, advertises tools/resources only, lists deterministic tools and the two capability-filtered templates, and does not claim unsupported task/subscription/list-change features.
- [ ] `resources/list` is empty and `resources/templates/list` is deterministic; `resources/read` returns the exact resource protocol shape and narrow error categories with stable Krometrail data.
- [ ] Root pointer-identity tests prove MCP receives the already-shared bundle/progressive/context services and the one concrete store/artifact authority; no MCP-specific cache, decoder, or payload map exists.
- [ ] Stdio EOF, SIGINT, and SIGTERM converge through the existing session shutdown exactly once; temporal calls are cancelled without corrupting retained evidence, and no non-JSON-RPC stdout output is emitted.
- [ ] Control-only MCP configuration remains fully functional and temporal-disabled configuration publishes no temporal tools or resource templates.

### Unit 5: integrated local workflow qualification

**Story:** `epic-temporal-debugging-workflow-mcp-investigation-surface-qualification`

**Files:**

- `crates/krometrail-mcp/tests/protocol.rs` (new or existing MCP protocol test location)
- `crates/krometrail-mcp/src/{registry.rs,response.rs,resources.rs,server.rs}` tests
- `src/app.rs` tests
- existing temporal bundle/progressive/store fixtures and focused integration tests
- `tests/rust-runtime-smoke.rs` only for truthful MCP stdout/lifecycle coverage

Use one deterministic local schema-v5 recording fixture and fake MCP/browser session over Tokio duplex streams. The scenario records a browser interaction, invokes `temporal_debug_bundle` with the interaction anchor, verifies exact resolved range/cache outcomes/capture gaps/event proximity, follows an artifact link through `resources/read`, requests a region filmstrip and selected source frames, reads a source-frame resource, pins the resolved range, queries verbose browser events, unpins it, and verifies deletion/eviction failures. Use barriers rather than sleeps for cancellation, resource eviction, and session deletion races. This is a deterministic product qualification and does not invoke paid agents or the separate thesis benchmark.

**Acceptance criteria:**

- [ ] JSON-RPC initialize/tools/list/resources/templates/list/tools/call/resources/read traffic is valid rmcp 0.11 traffic at protocol `2025-06-18`; all stdout lines remain protocol frames.
- [ ] The interaction-to-bundle path proves one natural resolution, exact resolved-range propagation, generated then cache-hit artifacts, compact correlated events, explicit gaps/degradations, one primary image or honest inline-limit degradation, and resource links.
- [ ] Region, source-frame, artifact-variant, verbose-event, and pin tools use the exact already-resolved scope/range, preserve deterministic ordering, and expose no diagnosis/causality claim.
- [ ] Resource reads return original retained bytes with exact MIME/hash/length, and eviction, invalidation, wrong scope, deletion, cancellation, and malformed URI cases return no stale or partial content.
- [ ] Capability tests prove temporal-vision and browser-events filtering is independent from ordinary control registration; disabled event presentation does not remove the bundle route.
- [ ] Root tests prove one service/store/cache authority and no store mutation gate is held across resource file I/O or image generation.
- [ ] Rust 1.85 locked format/check/test/Clippy gates pass. Tests target public protocol/resource/lifetime contracts, not every registry branch, URI string permutation, trivial getter, SQL statement, or large image golden.

## Implementation order

1. `epic-temporal-debugging-workflow-mcp-investigation-surface-contracts-registries-and-resource-read`
2. `epic-temporal-debugging-workflow-mcp-investigation-surface-routing-session-and-cancellation` — depends on Unit 1
3. `epic-temporal-debugging-workflow-mcp-investigation-surface-response-resources-and-inline-evidence` — depends on Unit 2
4. `epic-temporal-debugging-workflow-mcp-investigation-surface-resource-server-and-root-composition` — depends on Unit 3
5. `epic-temporal-debugging-workflow-mcp-investigation-surface-qualification` — depends on Unit 4

These are sequential design/verification checkpoints for one cohesive feature owner, not five worker assignments. Contract/schema/resource-read work must settle before routing; routing must settle before response projection; response/resource semantics must settle before protocol/root wiring; qualification is last.

## Simplification and elimination

- Keep one MCP server, one stdio lifecycle, one capability selection, one common response envelope, and one shared cancellation bridge.
- Keep domain bundle/progressive/context services separate because their ports have distinct authorities and failure semantics; do not create a generic temporal facade merely to force one handler shape.
- Reuse `ArtifactHandle`, `ArtifactEvidenceHandle`, `SourceFrameHandle`, `ArtifactManifest`, `TemporalContext`, `PinState`, and `KrometrailError`; do not copy them into MCP DTOs.
- Add only the scoped source-frame read needed by URI identity. Do not add a payload table, resource lease, URI field to core, path resolver, session-global byte cache, or a synthetic resolved-range lookup.
- Use `ResourceLink` and `resources/read` for durable evidence. Do not expose full artifact/frame bytes in structured tool JSON, duplicate JSON in text, or use unsupported file references.
- Keep resource subscriptions, concrete resource enumeration, remote transports, task semantics, comparison, replay, diagnosis, framework/page-state tools, and paid multimodal evaluation outside this feature.

## Testing strategy

- **Registry/schema interface:** compare routes, names, descriptions, enabled capability membership, annotations, schemas, and resource-only exposure to the domain registries. Fail closed on drift.
- **Resource lifetime interface:** use one real store fixture and controlled eviction/deletion barriers to prove scoped reads never publish stale bytes and resource errors preserve stable codes/recovery.
- **Response interface:** test one bundle, one partial bundle, one generated artifact, one region result, one source list/fetch, one verbose event page, one pin state, one invalid input, and one cancellation. Do not duplicate response tests for every operation variant.
- **Protocol interface:** exercise initialize, tools/list, resource templates/list, tools/call, resources/read, malformed requests, visible errors, and EOF over rmcp duplex streams. Keep a small binary stdout smoke test.
- **Composition interface:** prove the root injects the existing service instances and that current-reference geometry delegates to the active session without a second session owner.
- **No low-value tests:** do not snapshot every URI permutation, every enum branch, SQL text, large image bytes, or implementation-private router maps. Existing temporal-vision PNG hashes and store lifetime tests remain the lower-layer authorities.

## Risks and rollback

- **Resource handles can expire between tool and read.** This is expected weak-handle behavior, not a reason to pin every result. Strict scoped revalidation and resource-not-found errors prevent stale reads. If an agent needs continued availability, it uses the explicit pin tool.
- **rmcp 0.11 resource APIs may be narrower than later SDK examples.** The design uses only the inspected `ServerHandler` resource methods and model types (`ResourceTemplate`, `ResourceContents`, `RawResource`, `ReadResourceResult`, and `ServerCapabilities::enable_resources`). If an implementation-only helper is absent, construct those public model values directly; do not raise the SDK version or claim later protocol features.
- **Inline image retrieval adds a second read after bundle generation.** It is intentionally bounded and uses the same progressive authority. A failed inline read degrades presentation; it never causes a valid bundle to be replayed or a new generation to occur. Resource links remain the progressive path.
- **Large source-frame fetches can pressure MCP context.** The adapter imposes four/4 MiB/16 MiB inline caps while resources retain larger bounded reads. It returns links rather than silently truncating or embedding an arbitrary subset.
- **Verbose event pages may be incomplete.** The exact chronological cursor, matched/returned counts, collection gaps, unavailable ranges, and warnings remain in `TemporalContext`; MCP does not relabel proximity as causality or claim completeness.
- **Capability selection and storage collection are independent.** Disabling browser-event presentation does not delete recorded events or make the temporal bundle fail. If later product policy requires stronger separation, it can change route membership without changing source/resource authorities.
- **Root and MCP signatures overlap with the completed control surface.** This is prepublic work: change the existing constructor directly and update all call sites in one design implementation. No compatibility constructor or parallel service is retained.
- **A source-frame URI has no range.** The new scoped read request is the explicit fix. Fabricating a one-frame `ResolvedRange` would weaken provenance and could make a missing source look like a valid range result.

## Pre-mortem

The most damaging failure is a resource URI that works after the original evidence has been evicted by reading a stale path or segment address. The design prevents this by keeping URIs opaque and typed, routing every read through `ProgressiveEvidence`, and requiring final store validation before bytes are returned. The observable fallback is an unavailable-resource error with recovery guidance.

The next failure is presenting a full artifact as a compact image or presenting an event as causally responsible for a visual change. The fixed inline byte ceiling, unchanged-byte policy, nested evidence posture, exact proximity reasons, and non-diagnostic summaries keep presentation bounded and honest. A too-large or unavailable primary becomes a degraded link-only response.

The least certain area is client behavior around resource templates and `ResourceLink` content in rmcp 0.11. The implementation qualification must use real wire messages, not just Rust model serialization. If a client ignores templates, the links remain explicit and the agent can call `resources/read` with the canonical URI; no file URI fallback is introduced.

## Integrated implementation evidence

The five sequential checkpoints are implemented and verified as one feature:

- Contracts/registries/resource reads: validated progressive resource-only operations, schema-derived routes, scoped lifetime checks, and schema-v5 store coverage.
- Routing/session/cancellation: capability-derived routes, one request cancellation/deadline bridge, current-reference geometry through the single session owner, and exact typed dispatch.
- Response/resources/inline evidence: byte-free structured projections, canonical links, exact blob projection, deterministic bounded images, and stable resource errors.
- Resource server/root composition: MCP 2025-06-18 tools/resources capability, empty concrete listing, deterministic templates, strict `resources/read`, shared runtime authorities, and EOF/signal lifecycle preservation.
- Final qualification: real rmcp 0.11 JSON-RPC over Tokio duplex covers initialize, tools/resources listing, templates, calls, resource authority, malformed/unavailable/cancelled reads, control-only filtering, exact drill-down route propagation, and EOF.

Feature-level verification:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (636 passed, 1 ignored)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

The feature is advanced to `stage: review`; no parent review was performed in this implementation pass.

## Blockers

None. The feature is ready for its separate parent review; paid/thesis evaluation and other explicitly out-of-scope capabilities remain deferred.

## Foundation/document posture

This is a code-first foundation change. The foundation documents already describe temporal tools, progressive resources, local-only data, explicit errors, and the MCP boundary as intended future contracts; this design does not make a foundation assertion false before implementation. Documentation updates, if needed after implementation, belong in the implementation commit rather than this design transition.

## Review fix evidence

Accepted Important finding fixed without changing the feature stage:

- Root cause: `BrowserEventDetailRequest` reused the broader `TemporalContextRequestWire` schema even though its deserializer accepted only chronological selection.
- Fix: the core now owns one chronological-only `BrowserEventDetailRequestWire` for both validation and generated schema; compact input has no compatibility path and is rejected before the context port is called.
- Regression coverage: the core schema assertion proves `chronological` is present and `compact` is absent; the MCP JSON-RPC route test proves a compact `query_browser_events` request returns an error with zero additional application calls.
- Verification with Rust 1.85: locked workspace check, test (636 passed, 1 ignored), clippy with `-D warnings`, and format check all pass. No review was rerun.

Nonblocking advisory comments retained for later consideration, not expanded in this fix:

- Internal projection message wording.
- Named constants for existing literals.
- The unreachable `u32` size conversion path.
- The stale test name.

## Review decision

**Approved.** The single standard independent review was performed by `zai/glm-5.2` after implementation by `openai-codex/gpt-5.6-luna`. The chronological-schema finding was accepted and fixed in `245fb1f`; focused regressions and the complete Rust 1.85 workspace gate verify the repair. The remaining comments are non-material current-cycle observations and do not block completion. Per standard review policy, no repeat review was run. The feature advances to `done`.
