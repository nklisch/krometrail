---
id: epic-agent-browser-operation-mcp-control-surface
kind: feature
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-waits-and-batches]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-14
---

# MCP Browser-Control Surface

## Brief

Expose the integrated control capability to coding agents over MCP stdio through composable lifecycle, page, observation, navigation, interaction, wait, evaluation, screenshot, and batch tools. Register only enabled capability tools, derive standalone and batch schemas from the shared Rust operation contracts, and return concise structured results with stable errors, interaction anchors, a context-sized post-action image when appropriate, and resource references for larger outputs.

Keep handlers thin: validate external input, invoke one application service, and map the domain result without embedding CDP commands, target logic, persistence, or image processing. This feature turns the reserved MCP crate into the agent-facing adapter and root-wires it; temporal investigation tools, durable artifact resources, and unavailable page/framework-state capabilities remain outside this epic.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: final consumer — exposes the completed browser-control operation set after waits and batching integrate both standalone families
- Inherited decisions: capability and action registries are single sources of truth; disabled capabilities contribute no tools; local stdio is the supported transport

## Simplification opportunity

- Generate schemas and registration from shared capability/action contracts and keep one response/error translator. Do not preserve the empty placeholder shape, add handwritten schema mirrors, or create an alternate CLI/daemon control runtime alongside MCP.

## Foundation references

- `docs/VISION.md` — Core Experience and Local-First Operation
- `docs/SPEC.md` — Browser-Control Surface, Capabilities, Current-State Observation, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Capability Registry, MCP Boundary, and Dependency Direction
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Current implementation map

- `crates/krometrail-core/src/browser/operation.rs` is the one macro-backed operation source: 24 kinds, typed request/result associations, stable names, scope, mutability, evidence, batch admission, and nine action definitions.
- `crates/krometrail-core/src/browser/{observation,control,interaction,wait,batch}.rs` already owns validated Serde requests and the result values. `BatchRequest.steps` reuses `BrowserOperationRequest`; there is no batch-only action enum.
- `crates/krometrail-core/src/ports/browser.rs` owns browser connect/session lifecycle. An active `BrowserSessionPort` exposes `status`, `execute`, and `stop`; the production CDP session serializes `execute` through its existing supervisor.
- `crates/krometrail-core/src/capabilities/mod.rs` validates enabled, disabled, unavailable, dependent, and duplicate capability selection. Every operation in this feature belongs to `CapabilityId::Control`.
- `crates/krometrail-mcp` is an empty reserved adapter; `src/{cli,app,main}.rs` currently root-wire only `doctor`. The root binary is already the sole composition root.
- The waits/batches dependency is implementation-complete at `stage: review`: its 24-operation registry, one-target batch contract, cancellation/deadline path, and real-browser evidence are the implemented inputs to this design.

Direct local reads were sufficient. The caller prohibited nested agents and peer mechanisms, and the core registry, production port, SDK source, and binary composition points made the remaining design questions concrete.

## Design decisions

- **Transport and SDK:** Pin official `rmcp = "=0.11.0"` with its required default server/macros/base64 feature set plus `transport-io`; the adapter does not use the macros, but this SDK release's server implementation does not compile when they are disabled. Client and HTTP transports remain disabled. The router is built with `ToolRouter::add_route` and `ToolRoute::new_dyn`, and stdio runs through `ServiceExt::serve(rmcp::transport::stdio())`.
- **Tool topology:** Expose the 24 existing operation variants as 24 standalone tools using their registry stable names (the `batch` variant is the ordered batch tool), plus four fixed lifecycle tools: `start_browser`, `attach_browser`, `browser_status`, and `stop_browser`. Lifecycle uses the existing connector/session lifecycle ports; it is not a second browser-operation or action enum.
- **Generated request contracts:** Add `schemars::JsonSchema` to the existing Serde wire contracts and generate each operation's schema from its request type in the operation macro. For validated custom deserializers, `JsonSchema` delegates to the already-existing private wire type so duration and tagged-union shapes cannot drift from deserialization.
- **Batch schema composition:** Generate the batch schema from `BatchRequest`, then filter the generated `BrowserOperationRequest` union by `BROWSER_OPERATION_REGISTRY.batchable` and enabled capability membership. Do not hand-copy the step variants. Runtime `BatchRequest` deserialization remains authoritative for semantic invariants.
- **Capability exclusion:** Every operation definition carries `CapabilityId::Control`; lifecycle descriptors carry the same capability. `McpConfig` validates one startup selection and the router never adds a disabled or unavailable capability's routes. Enabled capabilities with no implemented MCP definitions contribute no speculative tools.
- **Session ownership:** One MCP process owns at most one active `Arc<dyn BrowserSessionPort>` behind `BrowserSessionOwner`. Start/attach serialize slot replacement; operation calls clone the active handle; stop atomically removes then stops it; EOF, SIGINT, or SIGTERM performs the same bounded stop/detach path. No workspace singleton or alternate daemon is introduced.
- **Cancellation:** Extend `BrowserSessionPort::execute` with a core-owned `BrowserOperationContext` carrying an optional infrastructure-neutral `CancellationSignal`. The MCP adapter bridges rmcp's request token; CDP combines it with existing session stop/disconnect cancellation. This avoids dropping an `execute` future while the supervisor silently continues a mutation.
- **Responses:** All tools use one adapter-owned `ToolResponse` envelope and output schema. The result projection is operation-specific JSON, while status, stable error, interaction anchor, warnings, and image metadata remain stable top-level fields. Text content is a concise summary, not a duplicate JSON dump.
- **Images and resources:** Image bytes are never placed in structured JSON. Explicit screenshots and post-action/final observations become MCP image content with their existing PNG/JPEG bytes and metadata; explicitly requested batch step screenshots may add the requested images. The adapter performs no resize, transcode, crop, persistence, or image analysis. No `ResourceLink` is emitted until a durable resource reader exists; temporal artifact/frame and durable interaction resources remain out of scope.
- **Visible failures:** Malformed tool arguments and all safe domain failures return `Ok(CallToolResult::error(...))` with the same structured stable error envelope. Unknown routes and adapter invariants that make the server unable to respond use rmcp protocol errors. A successful mutation with unavailable observation parts is `degraded`, not rewritten as a failed mutation.
- **Annotations:** Operation annotations derive conservatively from registry mutability: read-only operations set `readOnlyHint=true`, state-changing operations set `destructiveHint=true`, read-only operations set `idempotentHint=true`, and browser access sets `openWorldHint=true`. Lifecycle annotations are fixed with their lifecycle descriptors; `stop_browser` is destructive. No second per-operation annotation table is maintained.
- **Public command:** `krometrail mcp` is the only new executable surface. It emits no banner or logs on stdout; stdout belongs exclusively to newline-delimited MCP traffic and all diagnostics remain on stderr.
- **UI surface:** Inherit the parent epic's API-surface/no-human-UI decision. There are no screens or flows to mock.

## Architectural options

### Option A — one rmcp macro handler per tool

Declare 28 `#[tool]` methods with typed wrapper structs. This follows simple SDK examples and yields schemas automatically, but repeats the 24 operation names, request associations, metadata, and batch membership in the adapter. It optimizes for local familiarity at the cost of the epic's single-registry and generated-contract requirements. Rejected.

### Option B — dynamic registry adapter over the shared contracts (chosen)

Generate request schemas and request/result associations in core, build dynamic rmcp routes by iterating the operation and lifecycle definitions, inject the selected registry tag before deserializing the existing `BrowserOperationRequest`, invoke one cancellable `BrowserSessionPort::execute`, and translate through one response projector. It adds a small schema/filter layer but preserves the existing operation executor and keeps MCP as a Ports & Adapters boundary. This is the shortest sound design.

### Option C — one generic `browser_operation` MCP tool

Publish the full tagged `BrowserOperationRequest` union as a single tool. This is mechanically small and naturally preserves one schema, but removes the composable standalone surface, makes tool discovery and annotations coarse, and contradicts the parent/brief. Rejected.

## Architectural choice

Choose Option B. `krometrail-core` remains the authority for capability selection, operation identity, typed request validation, and request/result association. `krometrail-mcp` owns only MCP schema presentation, lifecycle ownership for the transport process, dynamic routing, and response projection. The root binary injects the already-constructed production `BrowserConnector`; no MCP code imports CDP, store, or temporal-vision crates.

The trickiest unit is generated schema/route composition, especially the recursive batch input. It is designed first below because a hand-maintained batch union would invalidate the entire single-source approach.

## Implementation units

### Unit 1: SDK qualification and generated wire schemas

**Likely files:**

- `Cargo.toml`, `Cargo.lock`
- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/browser/{operation,observation,control,interaction,wait,batch}.rs`
- `crates/krometrail-core/src/{ids,error,time}.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-mcp/Cargo.toml`

**Core signatures:**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserOperationDefinition {
    pub kind: BrowserOperationKind,
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
    pub evidence: OperationEvidence,
    pub scope: BrowserOperationScopeKind,
    pub batchable: bool,
    pub action: Option<&'static ActionDefinition>,
}

impl BrowserOperationKind {
    pub const ALL: &'static [Self];
    pub const fn stable_name(self) -> &'static str;
    pub fn input_schema(self) -> schemars::Schema;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", content = "request")]
pub enum BrowserOperationRequest { /* generated by define_browser_operations! */ }
```

The macro continues to emit kind/request/result/registry together. Each declaration adds only a nonempty agent-facing description; capability is generated as `Control` for this browser-operation registry. `input_schema()` uses Schemars draft 2020-12 settings and the declared request type. `ListPagesRequest` becomes an empty object struct so every MCP input schema has an object root and `{}` deserializes without a special adapter case.

`WaitRequest`, `WaitCondition`, and `BatchRequest` implement `JsonSchema` by delegating to their private integer-millisecond wire types. The same pattern applies anywhere custom Serde differs from Rust's field type. Validated scalars use transparent/string schema adapters rather than exposing UUID or nonzero implementation details.

**Dependency policy:**

```toml
# workspace
rmcp = { version = "=0.11.0", features = ["transport-io"] }
schemars = "1"
tokio-util = "0.7"
```

`rmcp` is exact-pinned because its protocol/router API is now a public executable boundary. Cargo.lock remains committed and all implementation commands use `--locked` after the intentional lock update. The crate publishes no MSRV, so this checkpoint must run `cargo +1.85.0 check --workspace --all-targets --locked` and a focused MCP test before downstream implementation proceeds. Failure keeps this story open; it is not solved by raising the workspace `rust-version` or silently choosing another SDK release.

**Acceptance criteria:**

- Every one of the 24 registry entries has exactly one nonempty description, `Control` membership, object-root request schema, and the existing stable metadata.
- Representative valid JSON values deserialize through the domain request and satisfy the generated schema shape; malformed duration, locator, batch, and validated scalar values still fail at Serde/constructor boundaries.
- Generated batch request schema can be inspected without infinite recursion.
- rmcp 0.11.0 compiles with its required default features plus `transport-io` on Rust 1.85 using the committed lock.

### Unit 2: Cancellable browser-session execution boundary

**Likely files:**

- `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/control/navigation.rs` and focused cancellation tests
- existing `BrowserSessionPort` fakes/call sites

**Core signatures:**

```rust
pub trait CancellationSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn cancelled(&self) -> PortFuture<'_, ()>;
}

#[derive(Clone, Default)]
pub struct BrowserOperationContext {
    cancellation: Option<Arc<dyn CancellationSignal>>,
}

impl BrowserOperationContext {
    pub fn with_cancellation(signal: Arc<dyn CancellationSignal>) -> Self;
    pub fn cancellation(&self) -> Option<&Arc<dyn CancellationSignal>>;
}

pub trait BrowserSessionPort: Send + Sync {
    fn session_origin(&self) -> SessionOrigin;
    fn status(&self) -> PortFuture<'_, Result<BrowserStatus>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> PortFuture<'_, Result<BrowserOperationResult>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}
```

The CDP `SupervisorCommand::Execute` carries this context. Its existing private `OperationCancellation` checks the request signal alongside session stop and disconnect in the same biased cancellation races used by navigation, waits, interactions, screenshots, and batches. Default in-process callers use `BrowserOperationContext::default()`; MCP supplies a real signal. No Tokio type enters core.

**Acceptance criteria:**

- Cancellation before dispatch sends no CDP command; cancellation during an operation returns stable `cancelled` with available target context.
- Cancelling one MCP request does not stop the whole browser session or cancel another queued request.
- Session stop/disconnect still wins through the existing cancellation path, including child batch execution.
- Every fake and production port deliberately accepts the context; no default method hides an uncancellable adapter.

### Unit 3: Capability-driven dynamic registry and one-session lifecycle owner

**Likely files:**

- `crates/krometrail-mcp/src/{config,schema,session,registry,server}.rs`
- `crates/krometrail-mcp/src/lib.rs`

**Adapter signatures:**

```rust
#[derive(Clone, Debug)]
pub struct McpConfig {
    enabled_capabilities: Arc<[CapabilityId]>,
}

impl McpConfig {
    pub fn new(enabled_capabilities: Vec<CapabilityId>) -> Result<Self>;
    pub fn is_enabled(&self, capability: CapabilityId) -> bool;
}

#[derive(Clone)]
pub struct BrowserSessionOwner {
    connector: Arc<dyn BrowserConnector>,
    active: Arc<tokio::sync::RwLock<Option<Arc<dyn BrowserSessionPort>>>>,
}

impl BrowserSessionOwner {
    pub async fn start(&self, request: LaunchBrowser) -> Result<BrowserStatus>;
    pub async fn attach(&self, request: AttachBrowser) -> Result<BrowserStatus>;
    pub async fn status(&self) -> Result<BrowserStatus>;
    pub async fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> Result<BrowserOperationResult>;
    pub async fn stop(&self) -> Result<BrowserStopOutcome>;
    pub async fn shutdown(&self) -> Result<()>;
}

#[derive(Clone)]
pub struct KrometrailMcpServer {
    sessions: Arc<BrowserSessionOwner>,
}

pub(crate) fn build_router(
    server: KrometrailMcpServer,
    config: &McpConfig,
) -> Result<rmcp::handler::server::router::Router<KrometrailMcpServer>>;
```

`start` and `attach` hold the write slot across `BrowserConnector::connect` so two lifecycle requests cannot create competing sessions. On success they obtain one initial `status` before publishing the session. Operation execution clones the active port and calls `execute` exactly once. `stop` removes the slot before awaiting the ownership-aware core stop; a concurrent operation either uses the prior handle and receives cancellation or sees no active session. No-session and already-active cases are `invalid_lifecycle_transition` errors with concrete start/stop recovery.

For each enabled operation definition, the registry creates `Tool::new(stable_name, description, input_schema)` with the common output schema and derived annotations, then adds a `ToolRoute::new_dyn`. The handler wraps the arguments as:

```json
{"operation":"<registry stable name>","request":{ /* tool arguments */ }}
```

and deserializes `BrowserOperationRequest`; this reuses the generated tagged enum and validated request types. The batch tool receives the existing nested tagged step form.

`schema.rs` starts from the generated `BatchRequest` schema and finds the generated operation union by its `operation.const` branches. It retains exactly the enabled definitions where `batchable == true`; missing, duplicate, or extra branches are an adapter initialization error. This makes the schema and runtime admission independently check the same registry metadata without a second variant list.

**Acceptance criteria:**

- Default control registration yields exactly the four lifecycle names plus all 24 operation stable names, once each, in deterministic MCP list order.
- Disabling `Control` omits every lifecycle and operation route; unavailable/invalid selections fail before serving.
- Every standalone schema comes from its domain request type; the batch step schema names equal the enabled `batchable` registry set.
- One valid call reaches exactly one fake `BrowserSessionPort::execute` with the expected generated request and cancellation context; invalid input reaches it zero times.
- Starting/attaching twice, status/stop without a session, stop/action races, and ownership-aware stop/detach are explicit and tested.

### Unit 4: Stable structured/error/image response projection

**Likely files:**

- `crates/krometrail-mcp/src/response.rs`
- `crates/krometrail-mcp/src/registry.rs`
- focused MCP response tests

**Adapter signatures:**

```rust
#[derive(Clone, Copy, Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolResponseStatus {
    Succeeded,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ResponseImage {
    pub role: ImageRole,
    pub metadata: ScreenshotMetadata,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolResponse {
    pub tool: String,
    pub status: ToolResponseStatus,
    pub result: serde_json::Value,
    pub interaction: Option<InteractionAnchor>,
    pub warnings: Vec<KrometrailError>,
    pub images: Vec<ResponseImage>,
    pub error: Option<KrometrailError>,
}

pub(crate) struct MappedResult {
    pub response: ToolResponse,
    pub summary: String,
    pub images: Vec<EncodedMcpImage>,
    pub is_error: bool,
}

pub(crate) fn map_operation_result(
    result: BrowserOperationResult,
) -> std::result::Result<MappedResult, ResponseInvariantError>;

pub(crate) fn into_call_tool_result(
    mapped: MappedResult,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData>;

pub(crate) fn visible_error(tool: &str, error: KrometrailError)
    -> rmcp::model::CallToolResult;
```

The mapper has one exhaustive compiler-checked match over `BrowserOperationResult`, but contains no operation names, schemas, dispatch functions, or capability membership. Repeated page-operation and interaction variants call family helpers. Screenshot projection emits only `ScreenshotMetadata` in `structuredContent`; bytes become `ContentBlock::image(base64, mime)`.

Outcome mapping is explicit:

- ordinary typed results and successful page/interaction results: `succeeded`;
- successful state change with unavailable page/snapshot/screenshot evidence: `degraded`, stable warning(s), no fabricated image;
- `PageOperationOutcome::Failed`, `WaitOutcome::TimedOut`, and batch failed/stopped/cancelled/timed-out outcomes: `failed` with the partial typed result retained;
- top-level `KrometrailError`, including wire validation and no-active-session: caller-visible `failed` tool result;
- response invariant/serialization failure: safe stderr log plus rmcp internal protocol error.

Default standalone state-changing responses emit at most the post-action screenshot. `take_screenshot` emits its requested image. A batch emits its final observation image and adds step images only when `include_step_screenshots` was explicitly requested. Read-only inspection, snapshot, evaluation, list/status, and waits do not invent screenshots. No output advertises a resource URI that cannot be read.

**Acceptance criteria:**

- Structured success, degraded, and failure envelopes conform to one advertised output schema and preserve stable codes, retry/recovery, context, anchors, and image metadata.
- Text is a bounded one- or two-line summary and does not serialize the complete snapshot/evaluation/batch JSON again.
- PNG/JPEG bytes appear only in MCP image blocks with the correct MIME; structured JSON and logs contain no base64 payload.
- Representative page, interaction, wait, batch, observation-degradation, invalid-input, and top-level error cases are covered without one trivial test per operation.

### Unit 5: stdio lifecycle and root binary wiring

**Likely files:**

- `crates/krometrail-mcp/src/{server,lib}.rs`
- `src/{cli,app,main}.rs`
- `tests/rust-runtime-smoke.rs`
- `docs/guide/mcp-configuration.md`, `docs/reference/runtime.md` (code-first truth update after the command exists)

**Runtime signatures:**

```rust
pub struct McpService { /* Router<KrometrailMcpServer> + session owner */ }

pub fn build_service(
    connector: Arc<dyn BrowserConnector>,
    config: McpConfig,
) -> Result<McpService>;

impl McpService {
    pub async fn serve_stdio(self) -> Result<()>;
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Command {
    Doctor,
    /// Serve Krometrail browser-control tools over MCP stdio.
    Mcp,
}
```

`serve_stdio` calls `router.serve(rmcp::transport::stdio())`, captures the returned running-service cancellation handle, and spawns one signal waiter for SIGINT/SIGTERM. EOF or a signal terminates the rmcp loop, then `BrowserSessionOwner::shutdown` invokes the existing bounded ownership-aware stop/detach. Signal task cleanup is explicit. Startup/serve/join/shutdown errors map to safe `KrometrailError` values for the existing root error reporter.

No code in the MCP path calls `println!`. The root's existing `doctor` text behavior remains unchanged. Tracing/log initialization, if added, must explicitly target stderr and must not install rmcp protocol logging on stdout.

**Acceptance criteria:**

- `krometrail mcp` completes MCP initialize/list/call framing over stdio with no non-protocol stdout bytes.
- stdin EOF exits cleanly; signal cancellation closes the rmcp service and then closes/detaches the active browser session once.
- Help and runtime smoke tests show the truthful `mcp` command while preserving `--version`, no-argument help, and `doctor` contracts.
- No alternate binary, socket, HTTP transport, daemon, or direct CDP construction is introduced.

### Unit 6: protocol and workspace qualification

**Likely files:**

- `crates/krometrail-mcp/src/*` unit tests
- `crates/krometrail-mcp/tests/protocol.rs` or equivalent in-memory protocol test
- `tests/rust-runtime-smoke.rs`

Use a fake connector/session plus Tokio duplex streams to perform real JSON-RPC initialize, `tools/list`, and representative `tools/call` messages against the rmcp service. This protects the negotiated protocol and framing rather than only calling Rust handlers. A binary subprocess smoke covers actual stdin/stdout ownership and EOF.

**Acceptance criteria:**

- Registration/schema exhaustiveness checks compare the listed tools and batch branches to `BROWSER_OPERATION_REGISTRY`, not hard-coded 24-name snapshots.
- Typed request round trips and boundary rejection cover one simple request, one validated interaction, wait durations, and nested batch composition.
- Response tests cover success/error/degraded/image/anchor mapping and disabled capability omission.
- In-memory protocol smoke initializes, lists tools, executes one valid operation against a fake session, receives one visible invalid-input error, and shuts down without leaked tasks.
- Binary smoke asserts every stdout line under `mcp` is valid JSON-RPC and diagnostics/help do not contaminate it.
- `cargo +1.85.0 check --workspace --all-targets --locked`, format, locked workspace check/test, and Clippy with `-D warnings` pass. Existing browser-control tests remain green.

## Implementation order and child checkpoints

```text
generated-contracts-and-sdk
        |
        v
cancellable-execution
        |
        v
registry-and-session
        |
        v
response-mapping
        |
        v
stdio-wiring
        |
        v
qualification
```

1. `epic-agent-browser-operation-mcp-control-surface-generated-contracts-and-sdk`
2. `epic-agent-browser-operation-mcp-control-surface-cancellable-execution`
3. `epic-agent-browser-operation-mcp-control-surface-registry-and-session`
4. `epic-agent-browser-operation-mcp-control-surface-response-mapping`
5. `epic-agent-browser-operation-mcp-control-surface-stdio-wiring`
6. `epic-agent-browser-operation-mcp-control-surface-qualification`

The six stories are dependency/verification checkpoints for one cohesive feature owner. They are not default parallel agent assignments: core schema generation, the session port, the dynamic router, response mapper, and protocol tests share one public contract and should remain in one implementation context.

## Test approach

- **Schema/registration interface:** Compare enabled tool names, schemas, annotations, object roots, and batch branch membership to the shared registries. This is the primary drift guard.
- **Wire validation:** Round-trip a representative set through the exact MCP arguments → tagged domain request path and assert invalid semantic inputs never reach the fake port.
- **Response interface:** Test one result from each genuinely distinct response family, especially degraded observation, stable error, interaction anchor, screenshot metadata/bytes separation, wait timeout, and partial batch failure. Do not duplicate a test for every operation variant that shares a family mapper.
- **Lifecycle/cancellation seam:** Fake connector/session tests assert one active owner, one execute call, per-request cancellation, stop/detach on transport exit, and no cross-request cancellation.
- **Protocol and binary smoke:** Exercise rmcp over duplex and the real executable over pipes. These catch framing, handshake, stdout contamination, and shutdown behavior that direct handler tests cannot.
- **No test removal:** The current runtime smoke contracts remain valuable. Extend them rather than replacing them with MCP-only assertions.

## Simplification

- Replace the reserved one-line MCP placeholder with cohesive adapter modules; do not preserve a compatibility facade for an API that never existed.
- Reuse `BrowserOperationRequest`, `BatchRequest`, capability validation, the production connector, and `BrowserSessionPort`; no MCP request enum, batch action enum, browser manager, or direct CDP router is added.
- Generate request schemas from existing wire types and filter the generated batch union from registry metadata. The only exhaustive result match is the necessary image-aware adapter translation, not a routing or schema registry.
- Keep resource support absent rather than adding a resource registry with no readable durable object behind it.
- Keep one `krometrail` binary and one Tokio runtime. `mcp` is a command branch, not a second executable or daemon.

## Pre-mortem and risks

- **Qualified older official SDK:** Exact rmcp 2.2.0 and every probed release through 0.12.0 failed Rust 1.85; 0.11.0 is the newest probed official release that passes. It supports the needed dynamic routes, structured content, images, request cancellation, stdio, and MCP 2025-06-18 contracts but not later task metadata. The adapter exact-pins 0.11.0 and explicitly qualifies the real workspace lock under Rust 1.85.
- **Schema/Serde divergence:** Deriving schemas directly on Rust fields would misdescribe custom integer-millisecond durations and validated transparent values. Delegating `JsonSchema` to existing private wire structs and schema-vs-deserialization tests is the fallback-safe design.
- **Recursive batch schema drift:** Schemars layout is not a stable API. The filter must identify operation branches by their generated `operation.const` values, verify exact registry coverage, and fail server startup on an unexpected layout rather than publish a permissive or stale schema. Runtime domain validation remains the safety net.
- **Cancellation after dispatch:** Dropping the MCP future without a port signal could allow a hidden mutation. The explicit cancellation context is therefore required before dynamic calls ship; returning early while execution continues is not an allowed fallback.
- **Large current-state outputs:** Snapshot and evaluation contracts are already bounded by their domain/CDP layers, but they can still be substantial. The adapter avoids duplicating them in text and never puts image bytes in JSON. Durable resource drill-down is intentionally deferred rather than fabricated.
- **Session shutdown races:** EOF/signal, `stop_browser`, and an in-flight operation can meet concurrently. Removing the slot before stopping, idempotent owner shutdown, per-request cancellation, and ownership-aware `BrowserSessionPort::stop` are the single convergence path.

## Research references

Verified against current local source on 2026-07-14:

- Official SDK repository: <https://github.com/modelcontextprotocol/rust-sdk>
- rmcp 0.11.0 documentation: <https://docs.rs/rmcp/0.11.0/rmcp/>
- MCP specification supported by the selected SDK: <https://modelcontextprotocol.io/specification/2025-06-18>
- Local crate source: `/home/nathan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rmcp-0.11.0/`
- Exact APIs inspected: `src/handler/server/router/tool.rs` (`ToolRouter::add_route`, `ToolRoute::new_dyn`), `src/handler/server.rs` (visible tool failures versus protocol errors), `src/model/tool.rs` (input/output schemas and annotations), `src/model/content.rs` and `src/model.rs` (text/image/structured results), `src/transport/io.rs` (`stdio`), and `src/service.rs` (`ServiceExt::serve`, running-service cancellation/waiting).
- The published crate metadata has no declared `rust-version`; Rust 1.85 compatibility is therefore an implementation qualification gate, not an assumption.

## Out of scope

- Temporal range inspection, storyboards, difference maps, source frames, or artifact resources.
- Durable interaction/resource persistence or SQLite/store changes.
- Browser-event inspection MCP tools.
- `page-state` or `framework-state` tools while those capabilities remain unavailable.
- HTTP/SSE/WebSocket MCP transport, remote listening, authentication, or a second daemon/binary.
- Direct CDP commands, target resolution, SQL, image processing, or retention logic in MCP.
- Implicit network-idle behavior, replay, rollback, cross-target batches, or changes to the existing 24-operation semantics.

## Design notes

- Dispatch capability: highest capability selected by the autopilot caller because this establishes the first public MCP/stdio boundary, generated schemas, binary lifecycle, cancellation, and image/error behavior. Direct reads only were used because nested subagents and peeragent were explicitly forbidden.
- Review weight: `standard` from the autopilot default. This pass designs the feature; implementation receives the later feature-level independent review.
- Intentional lifecycle exception: start/attach/status/stop use their existing connector/session lifecycle ports and fixed typed descriptors. The 24 operation handlers each validate once and invoke exactly one cancellable `BrowserSessionPort::execute`; lifecycle is not forced into a fake operation enum merely to make the call graph uniform.
- Intentional resource omission: the brief's resource-reference shape is honored only when a readable resource implementation exists. This feature returns current screenshot images directly and does not promise temporal or durable resource URIs.

## SDK compatibility redesign (2026-07-14)

The first implementation checkpoint proved that exact rmcp 2.2.0 cannot satisfy the declared Rust 1.85 contract. A bounded descending probe found 0.11.0 to be the newest official release that compiles on Rust 1.85 with its usable server/stdio feature set. `bug-restore-rust-1-85-contract` separately restored the workspace baseline and is now terminal; this feature does not hide that pre-existing incompatibility inside its SDK lock update.

The selected 0.11.0 API preserves the design's load-bearing seams: `ToolRouter::add_route`, `ToolRoute::new_dyn`, typed input/output schemas, `CallToolResult.structured_content`, image content, request cancellation tokens, `ServiceExt::serve`, stdio transport, and running-service cancellation/waiting. The adapter explicitly advertises/negotiates the supported 2025-06-18 protocol contract and does not claim 2025-11-25 task support. Because 0.11.0's `ToolRouter::list_all` iterates a `HashMap`, the server sorts definitions by stable tool name before returning `tools/list`.

Implementation must also apply the accepted advisory corrections:

- Construct tool results from `CallToolResult::success` or `error`, replace content with the bounded summary/images, and assign `structured_content` directly so rmcp's convenience constructors cannot duplicate full JSON into text.
- Add an explicit per-request view/wrapper around session-shared operation cancellation. Cancellation is guaranteed before dispatch and between CDP round trips, waits, child steps, and post-action observation; an already-sent single CDP command cannot be unsent and may complete.
- Generate lifecycle schemas for `LaunchBrowser`, `AttachBrowser`, `ManagedProfile`, and `ProfileIdentity` through their validated wire shapes.
- Keep MCP on the existing full `Runtime` composition root because controlled browser capture needs recording and retention assembly even though handlers do not use storage directly.
- Expose each tool's inner request object to callers; inject the tagged `{operation, request}` envelope only inside the route handler.
- Return rmcp-compatible `Arc<JsonObject>` input schemas. Keep `BatchRequest<Vec<BrowserOperationRequest>>` as the one domain contract, with fail-closed Schemars layout assertions and committed-lock tests; do not add a second public step enum without concrete evidence.
- Use 0.11.0's actual running-service cancellation/waiting APIs rather than later SDK-only lifecycle helpers.

This compatibility revision changes only the SDK/protocol implementation choice. The 24 operation routes, four lifecycle routes, schema single-source policy, response semantics, stdio command, child dependency chain, and feature acceptance boundary remain unchanged.

## Implementation notes

- Execution capability: highest, selected by the autopilot caller because this feature establishes the public MCP/stdio contract, exact SDK lock, generated schemas, cancellation, image/error semantics, and process lifecycle. One feature owner implemented all six dependency checkpoints sequentially.
- Review weight: `standard` from the autopilot caller. Implementation stops at `stage: review`; the caller owns the independent feature-level pass.
- Files changed: workspace/core/CDP/MCP/root manifests and source; core/CDP/browser-control tests; MCP registry/response/protocol tests; binary runtime smoke; current runtime/MCP/configuration/privacy docs and regenerated `llms-full`.
- Tests added: registry/schema exhaustiveness, custom wire and recursive batch shape, per-request cancellation isolation, session lifecycle races, response degradation/error/image/anchor mapping, real rmcp JSON-RPC over duplex, and real binary stdio initialize/list/EOF/help contracts.
- Simplification: one core operation registry drives 24 names/descriptions/capabilities/schemas/routes/annotations; one session owner and one response projector replace any MCP-specific runtime/request/result mirrors.
- Discrepancies from design: exact rmcp 0.11 performs initialization before producing `RunningService`, so pre-initialize EOF is classified from `ServerInitializeError::ConnectionClosed`. Encoded image MIME is signature-derived because the existing result retains screenshot metadata but not encoding. The in-memory protocol test is crate-local to avoid widening production server visibility.
- Adjacent issues parked: none.

## Integrated verification evidence

- All six child checkpoints are `stage: done` with individual implementation evidence and commits.
- Exact `rmcp = "=0.11.0"` plus default server/macros/base64 features and `transport-io` remains locked and compiles under Rust 1.85.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passed.
- `cargo fmt --all -- --check` passed.
- `cargo check --workspace --all-targets --locked` passed.
- `cargo test --workspace --all-targets --locked` passed 418 tests across 38 suites.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed with no issues.
- Focused MCP qualification passed 9 tests; focused binary qualification passed 5 tests. JSON-RPC negotiated 2025-06-18, listed 28 stable sorted tools, dispatched one valid generated request exactly once, rejected malformed input before dispatch, kept stdout protocol-only, and shut down on EOF.
- `bun run docs:build` regenerated the public documentation corpus and completed the VitePress build.

## Design adjudications

- Kept the domain `BatchRequest<Vec<BrowserOperationRequest>>`; generated Schemars layout is filtered fail-closed against registry `batchable` metadata, with no second public step enum.
- Kept request cancellation as a core-owned infrastructure-neutral signal and a CDP request-scoped view over session stop/disconnect. Cancellation is guaranteed before dispatch and at round-trip, loop, wait, batch-step, and observation boundaries; a CDP command already sent cannot be retracted.
- Kept lifecycle outside the operation enum as four fixed typed routes over the existing connector/session ports.
- Kept `krometrail mcp` on the full root runtime so capture, recording, and retention assembly remain available to controlled sessions.
- Omitted resource links because no durable readable MCP resource exists in this feature; current PNG/JPEG bytes are image content only.

## Review (2026-07-14)

**Verdict**: Request changes

**Blockers**: A batch whose steps all succeeded but whose final live observation is incomplete is represented by the domain as `CompletedWithFailures`; MCP currently promotes every such outcome to `Failed`/`isError=true`. This contradicts the feature's successful-mutation-with-incomplete-evidence contract and can prompt an agent to replay already-applied actions. Map final-observation-only degradation to `Degraded` while retaining the domain outcome; reserve `Failed` for actual step/termination failures. Standard closure needs fix verification only, not another independent pass.
**Important**: `idea-mcp-cancellation-protocol-regression` parks the valid lower-risk cross-layer cancellation test gap without blocking current delivery.
**Nits**: Default capability selection includes enabled capabilities without current MCP tools; no-session status remains an intentional lifecycle error; malformed screenshot bytes remain a defensive protocol error; permissive launch fields remain intentional.
**Rejected**: Claims of duplicated registries, unsorted tools, recursive-schema nontermination, leaked queued cancellation, double stop, or foundation drift were disproved by code and tests or were outside current implemented scope.

**Notes**: One standard cross-model GLM 5.2 pass reviewed `fff3cac..667bd9b` and reran Rust 1.85 workspace check/test/Clippy, focused MCP tests, binary smoke, and formatting. The receiver accepted one material current-cycle response-semantics correction, parked one lower-risk regression gap, and accepted all other implemented contracts. No second independent review will run under standard weight.

## Review remediation (2026-07-14)

`project_batch` now distinguishes actual step/termination failures from `BatchOutcome::CompletedWithFailures` caused only by incomplete final live evidence. It tracks whether any step failed or was skipped; when every step succeeded, final-observation warnings keep the structured domain outcome but map MCP status to `Degraded` with `isError=false`. This preserves already-applied mutations and avoids encouraging callers to replay the batch. Actual failed/stopped/cancelled/timed-out batches remain caller-visible failures.

The regression constructs one successful wait step plus an unavailable final observation and asserts `Degraded`, `is_error=false`, retained `completed_with_failures`, and one warning. Formatting, the focused MCP response test, the full Rust 1.85 all-target workspace check, and MCP all-target Clippy with warnings denied passed. The feature returned to `stage: review` for standard fix-verification closure; no second independent pass was required.

## Review closure (2026-07-14)

**Verdict**: Approve with comments

**Blockers**: none — final-observation-only batch degradation is corrected and verified.
**Important**: `idea-mcp-cancellation-protocol-regression` records the accepted lower-risk test follow-up outside the current cycle.
**Nits**: Default capability breadth, no-session status, malformed-image fallback, and permissive lifecycle fields remain intentional or defensive details.
**Rejected**: All unsupported registry, ordering, schema recursion, cancellation leak, double-stop, and foundation-drift proposals remain rejected by evidence.

**Notes**: Standard-weight closure used one cross-model pass, receiver adjudication, one material correction, and fix verification. No unresolved current-cycle risk remains and no second review pass ran.
