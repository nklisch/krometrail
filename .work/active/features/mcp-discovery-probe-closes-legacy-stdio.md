---
id: mcp-discovery-probe-closes-legacy-stdio
kind: feature
stage: implementing
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-06
updated: 2026-09-06
tags: [agent-ux, testing, infra]
---

# Modernize and qualify the MCP boundary

## Ownership and readiness

**Design complete; ready for parent implementation.** The user authorized end-to-end delivery and release, with this separate Astra xhigh design, parent-owned implementation, then **one comprehensive independent Astra review before release/publishing**. No extra designer/reviewer fanout is required. Stage remains `drafting` until implementation begins; no acceptance criterion below is claimed complete merely because its design is settled.

This one feature owns the direct official-SDK upgrade, modern and legacy wire qualification, bounded catalogue delivery/cache policy, and the small additional MCP corrections enumerated below. Keep the original report as point-in-time evidence. Its original capture-only authorization statements do not override the user's subsequent implementation/release authorization. Do not migrate the existing workflow: `.work/CONVENTIONS.md` does not declare Workbench ownership.

## Original finding

An independent Orogen MCP client cannot negotiate with Krometrail 1.6.3 using its default modern discovery-first path. Krometrail exits before sending a JSON-RPC response. A fresh process using legacy initialization first successfully negotiates and lists tools. This is a confirmed interoperability limitation of the selected older server/SDK, not a browser failure or evidence that a tool executed.

## Reproduction and observations

On Linux x86-64, launch the selected `krometrail mcp` binary with an empty inherited environment, disposable HOME/data/profile/temp roots, and separately supervised pipes. Keep stdin open. Send JSON-RPC request id `0`, method `server/discover`, with current protocol `2026-07-28` and empty client capabilities in request `_meta`.

Expected useful interoperability: bounded protocol rejection or negotiation that lets a client select the supported legacy exchange. Observed: no response, stdout closes, process exit `1` while its stdin is still open. Orogen rmcp 3.1.4 Auto reports `NegotiationFailed`; Orogen's owner reaps the child. No server JSON-RPC error code or tool-response correlation is available because no response was received.

An explicit separate diagnostic run sends `initialize` id `0` with requested `2025-11-25`. It receives `2025-06-18`, sends `notifications/initialized`, and receives 52 tools from `tools/list` id `1`. These are initialization/discovery operations, not tool calls. They authorize neither replay of a call nor silent process restarts.

## Version and source evidence

- Actual standalone binary: Krometrail 1.6.3, SHA-256 `a7b27d287d46beda5bf28376427d7f1784982bc8b8cd5128319d625e775565d3`.
- Public tag/source: `4d20efdc3487c7131496dba393e553e31bb51cfc`; `Cargo.toml` selects rmcp 0.11.0.
- Selected SDK source revision: `4c87f7f163000b95536308a8e594acd1b3f56b12`.
- [SDK server startup](https://github.com/modelcontextprotocol/rust-sdk/blob/4c87f7f163000b95536308a8e594acd1b3f56b12/crates/rmcp/src/service/server.rs#L185-L198) requires an InitializeRequest before returning a running server.
- In that same revision, `crates/rmcp/src/transport/async_rw.rs:119-127` converts a message decode error to end-of-stream; `:245-277` decodes the typed message and only tolerates selected unknown notifications, not arbitrary requests. Thus the earlier high-level initialize-first explanation does not prove that this unknown method reached the `ExpectedInitializeRequest(Some(...))` branch. A typed decode failure can terminate earlier. The exact internal error branch is source-supported but not independently instrumented in this release binary.

Private metadata-only Orogen receipts: `/storage/orogen-krometrail-wire-metadata.log` (original client wire), `/storage/orogen-fix-mcp-probe-2.log` and matching `.exit = 0` (independent probe confirms actual child exit). Orogen live failures remain in `/storage/orogen-krometrail-live-1.log` and `live-2.log`, `.exit = 101`. Probe success means the observation script completed, not server interoperability success. No browser was launched by these probes; no credential or page content was collected.

## Workaround and boundary

Orogen is implementing an explicit normalized `legacy_initialize` stdio selection using rmcp's existing Initialize lifecycle. Its default modern path remains available; no executable-name special case or automatic relaunch is proposed. Krometrail need not be patched or upgraded as a prerequisite for that bounded client path.

A future Krometrail change should deliberately select its supported protocol behavior and qualify discovery-first startup, rather than copying an SDK parser or claiming current-era support from legacy initialization alone. This capture authorizes no runtime change, dependency upgrade, public issue submission, or browser mutation.

## Separate client schema limitation

The advertised `oneOf` and `anyOf` constraints are legitimate inline JSON Schema, not a demonstrated Krometrail defect. Examples include `snapshot_page` at `$/properties/target` (`oneOf`), `create_page` at `$/properties/initial_url` (`anyOf`), and `fetch_source_frames` at the root (`oneOf`). Krometrail's schema projection resolves local references before publication. Orogen's former host subset rejected these keywords; that is being corrected in Orogen using its existing validator without removing constraints.

The complete public tool list is approximately 900,845 compact-JSON bytes (largest input schema 45,705 bytes), also exceeding Orogen's former 256 KiB aggregate discovery allowance. That limit and its misleading `InvalidToolCount` report are Orogen limitations, not an invalid Krometrail catalogue. No separate upstream schema bug is filed without evidence of an invalid advertised schema.

## Design evidence — 2026-09-06

### Verified upstream authority

GitHub's latest-release API independently returned stable **rmcp-v3.2.0**, published `2026-08-31T23:16:55Z`, not a prerelease. The designer inspected the actual tag commit **`51ccb42993d6eb5075399672ce7a0c21a0e55eea`**, not newer main. The parent's originally supplied clone path was absent in this worker; the tag was fetched to `/tmp/pi-github-repos/modelcontextprotocol/rust-sdk@rmcp-v3.2.0`. No SDK build was run.

Source anchors at that immutable commit:

- [Server dispatch, metadata validation, response-version projection](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/handler/server.rs#L51-L273): unsupported per-request version is `-32022`; required metadata is validated by the SDK; modern resource-not-found becomes `-32602`; old responses lose `resultType`.
- [ServerHandler defaults and supported versions](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/handler/server.rs#L315-L516): the default supported set is every SDK-known version; discovery derives from server info; `call_tool`/`read_resource` return response enums; unimplemented prompt listing/completion have successful defaults.
- [Opening exchange and legacy negotiation](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/service/server.rs#L448-L683): explicit supported-version override bounds legacy negotiation; a modern opener is processed without initialize; malformed/missing opening metadata gets an error response followed by startup failure. The first non-initialize handler is awaited before the concurrent service loop starts.
- [Handler scheduling, cancellation, and shutdown drain](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/service.rs#L1559-L1778): handlers are spawned independently; cancellation signals their context token and suppresses the ordinary eventual response, rather than aborting the handler; EOF drains responses for five seconds and explicit cancellation for two seconds. This is **not** application task joining or proof of completed browser/encoder/publication cleanup.
- [RunningService lifetime](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/service.rs#L1047-L1182): `waiting()` consumes the running service and its drop guard cancels tokens on drop; `close_with_timeout` does not turn an unfinished application cleanup into a completed one.
- [Discovery result constructors](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model.rs#L1178-L1282): discovery includes result type, required TTL/scope, and identity at `_meta["io.modelcontextprotocol/serverInfo"]`, not top-level `serverInfo`.
- [List/read result constructors and cache fields](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model.rs#L1547-L1801): `with_all_items`/`ReadResourceResult::new` default `resultType` but leave TTL/scope absent. The application must supply modern required cache fields.
- [Request metadata authority](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model/meta.rs#L11-L20) and [required keys](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model/meta.rs#L396-L567): use `RequestContext.meta`; wire `params._meta` is extracted there, not left in typed arguments. Protocol version and client capabilities are required; client identity is optional.
- [Flat resource models](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model/resource.rs#L5-L169) and [content models](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/crates/rmcp/src/model/content.rs#L1-L94): current `Resource`, `ResourceTemplate`, `ContentBlock` replace older raw/annotated wrappers; resource size is `u64`.
- [Manifest/MSRV](https://github.com/modelcontextprotocol/rust-sdk/blob/51ccb42993d6eb5075399672ce7a0c21a0e55eea/Cargo.toml#L6-L15): rmcp 3.2.0 declares Rust 1.88. This does not substitute for testing Krometrail's resolved lockfile on 1.88.

Normative pages inspected: [2026-07-28 versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/lifecycle), [caching](https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching), and [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools). The [3.0 migration discussion](https://github.com/modelcontextprotocol/rust-sdk/discussions/969) is useful background, but tagged 3.2 source wins over stale examples (including aliases and response-enum variants).

### Independent current catalogue measurement

The installed `/home/nathan/.cargo/bin/krometrail --version` returned `1.6.3`. An isolated, bounded metadata-only process initialized at `2025-06-18`, listed tools, then closed stdin and exited `0`. It used disposable HOME/temp/data roots, explicitly unavailable FFmpeg, and no inherited credentials; **no browser or tool execution was requested**. Its temporary state was removed. Compact UTF-8 JSON, no newline:

| Measurement | Observed |
| --- | ---: |
| Tools, with temporal video unavailable | 51 |
| Whole JSON-RPC tools/list response | 876,595 bytes |
| ListToolsResult alone | 876,561 bytes |
| Sum of input schemas | 204,928 bytes |
| Sum of output schemas | 659,328 bytes |
| Distinct output schemas | 1, repeated 51 times at 12,928 bytes each |
| Largest input / complete tool descriptor | generate_artifacts: 45,705 / 58,870 bytes |
| Next-largest input | generate_region_filmstrip: 24,207 bytes |

The original 52-tool/~900 KB report includes a different capability set; do not hard-code 52 or call the 51-tool measurement a lost route. Repeated output schemas account for about 75% of this payload. Catalogue pagination bounds individual messages but does not eliminate total wire/model context cost.

A read-only JSON transformation estimate factoring the duplicated `range`/`range_handle` branches saves **32,745 bytes** across eight non-video input schemas, including 18,173 bytes for `generate_artifacts` and 7,424 for `generate_region_filmstrip`. This estimate is not implemented or validation-qualified. It supports the limited schema correction below, not a claim of a major overall size reduction.

The parent separately reports `cargo test --workspace --all-targets --locked` passed before modification, using Rust/Cargo 1.98.0 and the existing shared `/storage/cargo-target`; baseline log `/tmp/krometrail-mcp-baseline.log`. The designer did not rerun it, create build targets, replay private Orogen receipts, or qualify modern runtime/client behavior.

## Settled implementation decisions

### D1. Upgrade the adapter directly; retain the architecture

- Set the workspace dependency to exact `rmcp = "=3.2.0"`; resolve the lockfile once. Prefer `default-features = false, features = ["server", "transport-io"]`: Krometrail uses its own registry/schema projection and base64 crate, not rmcp macros or base64 helpers. Add SDK client/child-process features **only to test dependencies** where used. Confirm feature closure with Cargo; do not retain old aliases to avoid compiler errors.
- Replace singular request `*Param` names with `*Params`; migrate manual server handler signatures to `CallToolResponse` and `ReadResourceResponse`; route completion results through `Complete`/`.into()` without adding tasks or input-request behavior. Preserve diagnostics on the inner complete result before returning the enum. Unexpected non-complete variants must not be reported as a successful tool execution.
- Replace old raw/annotated resource and content construction with current builders. Preserve each URI, MIME type, image bytes, resource link, size, and structured field. Use `u64` resource sizes directly instead of the old `u32` downcast. Keep the object-shaped `ToolResponse` and generated output schema, despite the SDK now allowing arbitrary JSON output.
- Use `ServerInfo`/`Implementation` builders/current fields and the existing Cargo version authority. Preserve resources and tools capability filtering; do not advertise prompts, completions, subscriptions, tasks, sampling, roots, logging, apps, or HTTP merely because the SDK contains them. Override successful SDK defaults for unimplemented `prompts/list` and `completion/complete` with bounded method-not-found responses; otherwise modern successful defaults would also need cache/result requirements the product does not intend to expose.
- Reject supplied `inputResponses` or `requestState` on tools/read requests with `-32602` before execution. The server never issues such continuation state, so silently ignoring it and executing a fresh mutation would misrepresent the request. Do not implement state sealing, authorization, or a continuation subsystem.
- No core/store/CDP dependency-direction changes, no alternate parser, no protocol envelope rewrite layer, no persisted recording-format change, no browser-session ID redesign. Browser ownership remains **application state belonging to one local process**, not state created by an MCP initialize handshake. Modern calls can use the same singleton owner without initialize.

### D2. Own the qualified version policy; let rmcp negotiate

One adapter-local constant lists exactly, in preference order:

1. `2026-07-28` — current modern protocol;
2. `2025-11-25` — current initialization-based protocol;
3. `2025-06-18` — existing Krometrail contract.

Override `supported_protocol_versions()` with precisely that list. Use explicit `2026-07-28` as the server's preferred info version, not `LATEST` or `KNOWN_VERSIONS`; rmcp selects `2025-11-25` for unsupported/newer initialize requests and echoes either qualified legacy version when requested. Older SDK-known versions are not separately advertised just because the library can deserialize them. Test both accepted legacy versions, not just fallback.

Modern requests carry `params._meta` with `io.modelcontextprotocol/protocolVersion` and `io.modelcontextprotocol/clientCapabilities`. Let the SDK validate, extract and select them. Do not require optional clientInfo or copy metadata into browser-tool argument schemas. Let the SDK strip `resultType` for legacy response serialization and remap modern resource-not-found; tests assert the actual wire, not manually serialized result structs.

Discovery may precede tools, but is not mandatory for modern calls. Qualify direct first-request `tools/list` as well. **Known SDK caveat:** a long first non-initialize request runs before its concurrent receive loop, so a cancellation notification queued behind that opener cannot be observed immediately. Do not invent a mandatory initialization handshake to hide this. Keep first action startup bounded by the existing connector limits, qualify cancellation after discovery, and state that cancellation is best-effort, never proof a first action did not run. Signal/EOF task lifetime is addressed independently in D5.

### D3. Build one immutable, bounded tool catalogue

Add a cohesive `catalogue.rs` beside the existing flat adapter modules. It projects the **actual configured router**, never a second list of tool identities.

- At server construction, sort descriptors by exact tool name once, retain the immutable collection, and compute deterministic page boundaries. Preserve optional temporal-video membership from the startup-qualified config; browser start/stop does not change catalogue membership.
- **Page limits:** at most **8 tools** and **192 KiB (196,608 bytes)** of compact serialized `ListToolsResult`, including cursor/cache/meta fields. Pack greedily in name order. Precompute against the larger modern shape; every legacy page must fit too. The bound is on the result, not arbitrary client-provided JSON-RPC ID length or a host's aggregate model-tool budget.
- Never split one tool descriptor or drop its output schema. If a future single tool cannot fit, keep the service alive but make tools/list report an actionable bounded catalogue error with the registered route identity and measured size, rather than quietly shipping an oversized page, a partial inventory, or refusing all MCP startup. This limits a concrete delivery failure to its owning surface; tests must prevent releasing that state. It is not a security guard, and the 192 KiB target is not claimed to be a protocol maximum.
- Cursors are opaque, bounded ASCII strings (maximum **160 bytes**), carrying a format marker, per-server-instance nonce, digest of the exact configured descriptors/page policy, and the next precomputed page start. One possible private format is `tools-v1.<instance-uuid>.<sha256>.<start>`. Decode with checked arithmetic and require a real nonzero next-page boundary; reject truncation, trailing data, oversized values, overflow, terminal/out-of-range/non-boundary indexes, another process/configuration's cursor, and unsupported cursor formats.
- Invalid cursors return `-32602` with static guidance: discard cached pages and restart `tools/list` without a cursor. Never echo a caller's cursor or derive filesystem access from it. These are snapshot positions, **not authentication tokens**, so no HMAC, durable cursor store, or compatibility decoder is needed.
- Repeated requests with the same cursor in one process return the same tools and continuation. Final pages omit `nextCursor`; an empty enabled set returns an empty final page. All enabled tools appear exactly once across a traversal. `resources/list` stays empty and templates remain a small complete list; because neither emits continuation cursors, any supplied cursor is rejected rather than ignored.
- Share each generated output schema `Arc` across its routes (ordinary vs video-role variants), and retain catalogue descriptors/pages rather than regenerating/cloning the entire catalogue on every list call. Do not add a second static catalogue file or disk cache. Measure construction cost separately from wire size.

**Schema decision:** keep inline, reference-free public schemas and all domain constraints. Factor only the `range`/`range_handle` exclusive choice: common properties and their required fields appear once at the root, add both selector properties, and use `oneOf: [{required:["range"]},{required:["range_handle"]}]` with root `additionalProperties:false`. Both-present fails both branches; neither-present fails both. All range/handle validators and other required fields remain unchanged. Add the shared `response` property once at that root; update the existing projection code/tests accordingly instead of retaining the obsolete duplicated branch shape. Handle any other genuine full-object union intentionally, not by indiscriminately adding properties to `oneOf` required-only branches.

This local factoring is justified by measured input duplication and simpler field discovery. It is not enough to solve a client's aggregate catalogue ceiling. Cross-tool `$ref` registries, omission of output schemas, constraint stripping, removal of descriptions, arbitrary per-client schemas, and broad schema-minification passes are rejected here. They either have no standard shared authority, weaken current contracts, or require a larger client-compatibility study. The 659 KB repeated output portion remains honest measured overhead.

### D4. Separate catalogue freshness from evidence availability

Use explicit result fields, not ad-hoc `_meta` cache keys. Apply the following on modern complete responses; omit optional cache fields for legacy list/read responses so their existing shape remains stable. SDK-owned resultType projection remains authoritative.

| Method | ttlMs | cacheScope | Reason |
| --- | ---: | --- | --- |
| server/discover | 0 | private | Re-establish exact executable/startup capability identity on discovery; it is cheap. |
| tools/list, every page | 60,000 | private | Immutable within this configured process; capability/encoder configuration is local, not a globally interchangeable catalogue. |
| resources/templates/list | 60,000 | private | Same startup-fixed capability boundary as tools. |
| resources/list | 0 | private | Intentionally no concrete evidence inventory; never imply current absence from a shared cached inventory. |
| resources/read, all kinds | 0 | private | Revalidate retention, process/session ownership and identity on each read, even for immutable bytes/manifests. |

No cache hints are invented for tools/call, errors or image blocks; those are not the cacheable list/read result contract. Preserve Krometrail's **internal validated artifact cache** independently of MCP client freshness.

Cache scope is not authorization, TTL zero is not an erasure command, and neither can make a client forget content it already read. Correct server behavior is to revalidate a fresh resource read and return current unavailable/evicted errors, not serve bytes from an adapter-side cache. Never make screenshots, browser events, downloads, manifests, range handles or diagnostics public merely because their identifiers are opaque.

Catalogue cache lifetime is bounded by **the exact process and startup configuration**: executable/version, selected capability snapshot (including qualified encoder availability), and page policy. A fresh connection/process must rediscover and discard previous pages; process-bound cursors detect stale continuation, not stale first pages. Private caches must not be keyed only by server name across differing launch configurations. The official rmcp client cache is per Peer; qualify that boundary. Document this expectation for other hosts rather than claiming `private` itself encodes process identity. No runtime config changes, polling, listChanged notifications or subscription surface are introduced. A client that caches all descriptors indefinitely in model configuration still needs an explicit host refresh/restart.

### D5. Preserve cooperative request cleanup across SDK and process exit

This is the riskiest seam. Existing `registry.rs` deliberately awaits video generation's cancellation/publication cleanup, while `server.rs` currently waits only for rmcp and then shuts down the active browser. Most in-memory tests call `service.server.serve(...)` directly, bypassing `McpService::serve_stdio` and OS-signal cleanup. These tests are necessary but insufficient.

Introduce a small adapter-owned `request_lifecycle.rs` (name may be shortened by parent) with **one shared request supervisor**:

1. Admit and register tool/resource execution before effects begin; catalogue/negotiation remains cheap SDK dispatch. Retain ownership of admitted execution futures in supervised tasks, with their cancellation tokens and completion tracking, independently of the SDK's response future. Dropping an MCP response waiter must not abandon encoder/publication or browser-lifecycle cleanup. Do not spawn a second protocol service or replay a request.
2. Keep request cancellation independent: cancelling one context signals only its work, not the active browser or unrelated calls. Preserve the existing `CancellationSignal` bridge and service-owned video cleanup. Do not wrap cleanup-owning effect futures in an aborting timeout/select merely to make a test finish. Do not add a blanket 30-second timeout to browser waits whose declared domain timeout can be 120 seconds.
3. Install signal handling **before awaiting the SDK opening exchange**. Use `serve_with_ct` and one process shutdown signal. If early EOF notification is needed, wrap the stdin `AsyncRead` only to report actual EOF/read failure, forwarding bytes unchanged; this is lifecycle plumbing, not a JSON parser or version negotiator. A reader wrapper only observes EOF when read is polled: rmcp's sequential opener can delay that observation until its bounded handler finishes. Do not claim otherwise or add an unbounded read-ahead pump. A valid first tool request still running when signal cancellation drops the SDK startup future remains owned by the application supervisor.
4. On EOF, transport termination, SIGINT/SIGTERM or startup failure: stop admission, mark the browser owner closing, cancel admitted request tokens, stop accepting new sessions, and drain owned application work and browser shutdown. Track SDK, request and browser cleanup separately; the SDK's five/two-second drain cannot certify the others. Always execute cleanup on startup-error paths, not only `ConnectionClosed`.
5. Use one absolute **30-second application shutdown deadline**, recorded at the first observed stop event and shared by all adapter cleanup waits; do not grant each request/phase another 30 seconds. Initiate request cancellation and browser shutdown together where ownership permits; account for time already used by the SDK. Preserve the existing CDP shutdown's own aggregate budget and ownership-aware force cleanup, not a replacement set of CDP close commands. Tests use injected short deadlines, not 30-second sleeps. Deadline exhaustion must be a non-clean result with bounded diagnostics, never `Ok(())` while tracked work or managed ownership remains. Last-resort task abort/drop is only after this cooperative drain and must retain the existing process/profile guards' cleanup authority; live process/profile assertions decide whether it is actually sufficient.
6. Ensure the browser owner cannot install a newly connected session after shutdown starts. Serialize start/attach/stop transitions, including an in-flight stop, and explicitly stop a candidate that fails its initial status check or loses to owner closing. Keep a stopping session/its completion owned until it is finished; taking the singleton slot and then dropping an unowned stop future is not adequate. Preserve the concrete structured BrowserStopOutcome and its degraded/closure distinction.

Use existing Tokio/tokio-util task tracking/oneshot primitives, not a generic job framework. No new public cancellation command or mandatory handshake. No unbounded queued application-worker abstraction. Additional concurrency quotas are not required by this design: current domain worker bounds remain authoritative; if implementation adds an admission cap, it needs explicit pre-dispatch failure semantics and evidence, not a speculative operational refusal.

**Review checkpoints:** first-request cancellation is best-effort because the SDK opener is sequential; OS shutdown can still signal its owned work. Per-request cancellation does not promise rollback or safe replay after dispatch. EOF is half-close (stdout may remain readable), not permission to wait forever for a request/child. A blocked output consumer must not defeat bounded process exit. In particular, dropping an async stdout future need not stop an already-running Tokio blocking I/O operation; if the backpressure test exposes runtime teardown waiting on it, resolve that narrow stdio/composition-root lifetime boundary rather than claiming the SDK timeout suffices. Attached Chrome survives detach/EOF; managed Chrome, profiles, encoders and staging files must not leak. If the existing lower-layer port cannot honor the above ownership/deadline boundary, record that concrete gap here and resolve narrowly rather than claiming the SDK fixed it.

## Additional small improvements outside modernization

These are distinct from the SDK, negotiation, pagination and caching changes. All are bounded adapter/description corrections, not a new automation architecture.

### F1. Reject ignored lifecycle arguments before acting — confirmed code defect

`registry.rs:982–1069` parses arguments for start/attach/status, but the stop and profile-list branches ignore their argument object entirely. `stop_browser` can therefore close the active browser even when the caller supplied an unsupported selector/option. The generated no-argument shape is not enforced by those manual routes.

Use one locally defined, closed no-argument wire type (`deny_unknown_fields`, generated schema) for `stop_browser` and `list_managed_profiles`. Deserialize before capture-health acquisition or lifecycle dispatch. Omitted arguments and `{}` are accepted; nonempty objects produce the existing model-visible invalid-input error, with no stop/profile port call. No guessing what unknown fields mean and no alias for a hypothetical session selector. This earns a regression at the dispatch boundary, not just a schema snapshot.

### F2. Make launch/attach honor the existing status detail preference — confirmed inconsistency

`lifecycle_route` adds `response` only to `browser_status`; start/attach serialize the whole `BrowserStatus` through `map_lifecycle_result`. Thus they reject the advertised cross-tool response preference and default to a much wider status shape than the explicit status tool.

Add the same generated optional `response` object to start/attach. Split it before decoding the unchanged core request, acquire status once, and project it through the existing `map_browser_status`. Default concise; expanded/full remain deliberate; no new acquisition path. Preserve selected target/session identity, ownership/profile, stride, open dialogs, capture/retention warnings, version and diagnostic outcomes. Inline-images preference does not fabricate an image for lifecycle status. This is an intentional caller-visible presentation correction, covered by release notes and tests.

### F3. Stop retaining arbitrary requested tool names in logs — confirmed privacy/bounds defect

`server.rs:154–159` copies raw `request.name` directly into the `mcp.request` tracing span before lookup. Unknown tool names are untrusted strings and need not be a bounded registered identifier. Default local logs should not retain arbitrary caller text under the guise of a route name.

Resolve the route against the existing router first; log only its canonical registered name, otherwise the static category `unknown_tool`. Return a bounded static unknown-tool protocol error without echoing the raw name. Use the same bounded request-diagnostic path for unknown custom RPCs instead of the new SDK default that uses the arbitrary custom method as error text. Preserve a local correlation ID and applicable diagnostic reference. Do not enable rmcp's broad request/result Debug logging to diagnose the migration; the existing `src/diagnostics.rs` target allowlist intentionally excludes it. Sentinel tests must cover oversized/control-character/secret-like names and unknown methods in wire diagnostics and default logs, while ordinary valid route identifiers remain useful.

### F4. Explain actual handle lifetime and offline resource discovery — confirmed guidance gap

The current range-resolution and generated-artifact descriptions do not tell a caller that handles/retained resources are process-scoped and evictable. `resources/list` is deliberately empty, and stop_browser's description does not explain that stopping capture does not itself delete this process's retained evidence. These are established runtime semantics, not a proposal for new retention.

Update the **owning registry descriptions** for resolve_temporal_range, temporal_debug_bundle and the relevant follow-up tools, plus resource-template descriptions and the short server instructions. State: one active browser per process; list_managed_profiles needs no active session; browser control needs start/attach; temporal queries can use this process's retained evidence after stop; range handles do not survive process restart; unpinned backing evidence may expire/evict; actual resource URIs come from tool results and templates, not an enumerated resource inventory; downloads additionally require their managed session to stay active. Keep this short and consistent with SPEC/skills; do not duplicate whole schemas into descriptions or encourage guessing URIs. Fix wording such as list_source_frames's blanket "expanded detail for full provenance" where full remains the wider tier; name the current detail/drill-down contract accurately.

### F5. Correct clearly local open-world hints — confirmed overbroad metadata

`temporal_annotations` and lifecycle profile listing currently mark everything `openWorldHint:true`. `list_managed_profiles`, retained source/artifact/event reads and retention pin operations interact with Krometrail's local owned catalogue/evidence, not an open external service.

Set closed-world hints for those known local surfaces by exhaustive projection from existing operation kinds (or compact metadata in their owning registry), never a second string-name list. Keep browser-control operations open-world and keep any operation that can resolve a current live-page geometry reference (notably region filmstrip) open-world unless its full accepted input surface proves otherwise. Do not broadly turn every browser interaction into non-destructive/idempotent work. Keep core-declared mutability, unpin's destructive hint, and actual idempotence semantics intact; this feature does not redefine whether internal cache publication counts as domain mutation. Exhaustive route tests pin these distinctions so future variants cannot inherit a misleading blanket hint.

### Explicitly not another new server fix

The source-confirmed Pi host suppression of nonempty structuredContent is already owned by `epic-a-grade-reliability-agent-result-delivery` and its Pi Extensions handoff. That item records accepted external source repair and separate publication/install/fresh-host status. Recheck the **installed bytes** during qualification; do not add unlimited JSON text duplication to Krometrail, claim an installation/reload that did not happen, or edit the external project under this design. Output-schema duplication, cursor handling and missing modern fields belong to D1–D4, not the additional-friction count. Broad new response-size policies, browser-status-without-session redesign, new prompts/apps, runtime capability switching and a new trace propagation system have no demonstrated need here.

## Implementation boundaries

| Area | Files / responsibility |
| --- | --- |
| SDK version/features | Root `Cargo.toml`, `Cargo.lock`; `crates/krometrail-mcp/Cargo.toml`; test-only client dependencies as needed. Retain Rust 1.88 compatibility; temporal-vision source/version remains untouched. |
| Protocol policy | New `crates/krometrail-mcp/src/protocol.rs`: qualified versions and deliberately small modern-result/cache policy helpers, no message parsing. `server.rs`: current handlers, discovery, correlation, unsupported methods and conversion. |
| Catalogue | New `crates/krometrail-mcp/src/catalogue.rs`: immutable descriptors, exact page accounting, bounded cursor parsing. `registry.rs` still builds the only route set. |
| Schemas | `schema.rs`: selector factoring, shared response property and no-argument schema. `registry.rs`: shared output-schema instances and lifecycle projection. Preserve generated domain validators. |
| Task/process lifetime | New `crates/krometrail-mcp/src/request_lifecycle.rs`, `server.rs` orchestration, narrow `session.rs` ownership/closing changes. `lib.rs` private module declarations. A narrowly evidenced root `main.rs` runtime-teardown correction is allowed if the blocked-stdio test requires it. Do not move lifecycle into core/store or reimplement CDP shutdown. |
| Responses/resources | `response.rs`, `resources.rs`: current SDK types, exact complete-result semantics, modern read cache policy at server boundary, strict no-cursor handling, concise launch/attach and metadata/guidance fixes. |
| Description authority | Existing definitions in `registry.rs`, `krometrail-core/src/timeline/query.rs`, `debug_bundle`/progressive/context operation registries as actually located; no second naming table. |
| Deterministic qualification | Extract shared process supervision to `tests/support/mcp_process.rs`; add focused `tests/mcp-protocol.rs` (or equivalent split); preserve `rust-runtime-smoke.rs` coverage and update every tools-list consumer to traverse pages. Move large newly touched adapter protocol tests into child test modules if it improves focus; avoid unrelated file churn. |
| Live qualification | New `tests/mcp-browser-live.rs` under `qualification-support`, using local browser fixtures and the actual binary. Share the bounded process harness, do not seed a flat store that startup deletes. Update existing `tests/video-mcp-live.rs` pagination and isolated-instance assumptions when exercising that test. |
| CI/release | Ordinary CI runs binary protocol tests without Chrome. Add a focused Linux/macOS MCP qualification workflow or extend the existing exact-ref/SHA gate without duplicating its full CDP campaign. Required live rows fail if Chrome/receipts are absent. Release artifacts get metadata-only stdio probes before publication, not only `--version`. |
| Current docs/plugin | Update SPEC MCP contract, ARCHITECTURE adapter boundaries, EVALUATION qualification matrix, manual MCP/troubleshooting/runtime guidance and shipped Krometrail skill where affected. Regenerate `docs/public/llms-full.txt` through docs:build. No historical migration essay or non-existent CLI examples. |

## Acceptance and verification plan

Every row below must have a named automated test or a versioned qualification receipt. A raw-wire assertion is distinct from successful SDK decoding; SDK result defaults can hide absent required fields.

### A. SDK and protocol wire matrix (default tests, actual executable)

- [ ] Fresh modern `server/discover` with required metadata returns `resultType:"complete"`, exactly the three supported versions, accurate capabilities/instructions, Cargo identity in namespaced `_meta`, `ttlMs:0`, `cacheScope:"private"`; stdin stays open and the same process serves the next request.
- [ ] Modern discovery followed by **all pages**, resource templates/list, a model-visible tool validation failure and a known-good tool such as list_managed_profiles. Every cacheable complete modern result includes both cache fields and resultType. No browser is launched by these rows.
- [ ] Direct modern first-request tools/list works without initialize/discovery. Modern requests do not depend on prior clientInfo; omitted optional clientInfo and unknown extension metadata are tolerated without leaking to tool arguments/logs.
- [ ] Legacy initialize separately at `2025-06-18` and `2025-11-25` selects that version; notifications/initialized remains supported; subsequent tool/list/read complete responses omit resultType and optional modern cache fields. Modern discovery and legacy init advertise the same configured routes/resources.
- [ ] Initialize requesting a newer/unsupported version falls back to the newest qualified legacy version (`2025-11-25`), not an untested SDK default. Legacy request metadata is not required. Per-request modern metadata, if tested on that connection, must not silently change later legacy request defaults.
- [ ] Fully formed unsupported-version modern opener returns `-32022` with exact `{requested,supported}` and can then accept a supported-version request on the same process. Unsupported version is not treated as a tool failure or a license to replay an executed action.
- [ ] Missing and malformed required metadata **after startup** return `-32602`; a following valid request succeeds. A malformed/missing-metadata **first opener** returns a bounded `-32602` before the SDK's bounded startup failure/exit. Do not demand same-process recovery where tagged SDK startup explicitly closes, and do not claim silent success.
- [ ] Unknown valid-metadata RPC opener/established request returns bounded `-32601` and keeps a viable session alive; unknown tool returns a bounded protocol error (`-32602` is the router's current code). No browser/domain dispatch. Unimplemented prompts/completions/tasks/subscriptions do not gain phantom success/capabilities.
- [ ] Malformed tools/call shape, unsupported continuation fields, invalid tool arguments and invalid catalogue cursors are distinguished. Tool-argument validation remains an `isError:true` complete tool result; protocol-shape/negotiation errors remain JSON-RPC errors. Degraded proven actions stay non-error, with original retry/warning/interaction evidence.
- [ ] Modern missing-resource error is `-32602`, legacy is `-32002`, with identical retained domain error/recovery/diagnostic data. Use a syntactically canonical unavailable URI, separately from malformed/disabled/wrong-scope identities.
- [ ] EOF before any message, after discover, after initialize, and after normal responses exits cleanly with only JSON-RPC on stdout. Pre-open SIGINT/SIGTERM uses the same bounded cleanup path. No indefinite `read_line`, pipe-fill deadlock or unreaped child in tests.

### B. Catalogue/schema/cache tests

- [ ] Across default, control-only, permitted disabled capabilities, and qualified/unqualified video: catalogue union equals router/registry membership, with exact order, no duplicates, no omissions, correct output schemas/annotations and no list change after browser lifecycle calls.
- [ ] Repeat each page; final/empty pages; malformed/too-long/overflow/foreign/stale/cross-config/non-boundary cursors; invalid cursors never become offsets. Assert every serialized modern and legacy **result** meets the item and byte bounds, including longest generated cursor. Test exactly-at/over-budget synthetic descriptors without weakening real-schema checks.
- [ ] Compile all public input and output schemas with a real draft-2020-12 JSON Schema validator in tests (dev-only dependency, MSRV-qualified). Keep no external references. Validate representative successful/degraded/failed structuredContent against the actual advertised output schema after diagnostics attachment.
- [ ] For every factored follow-up schema: valid full range, valid handle, common optional/default fields, required non-selector fields, response preference, both/neither selector, malformed/nil handle, invalid nested range, unknown root/nested fields, enum/oneOf/anyOf limits and boundary numeric values. Compare validator acceptance to the unchanged runtime decoder for a table of valid/invalid contracts; do not accept either success or failure.
- [ ] Preserve source-frame strict fetch/list pagination distinctions, batch operation filtering and runtime enum/schema parity. Do not narrow valid nested union constraints to satisfy a client subset.
- [ ] Record before/after catalogue bytes, input/output shares, largest tool, page count and largest page for still-only and video-enabled snapshots. No test asserts the historical tool count. The factoring estimate is an expected magnitude to explain differences, not a substitute for measured acceptance.
- [ ] Modern catalogue pages/templates have positive private TTL; discovery/empty resources/current retained reads have zero private TTL. Verify SDK cache reuse within one Peer and no reuse after a new process/Peer or changed configuration. Disable client stale-on-error/cache in wire-error tests so cached success cannot hide a server regression; test default positive caching separately.
- [ ] Read retained bytes then evict/delete backing evidence and repeat through the same actual caching client: the read must reach the authority and fail, not return client-cached data. After restart, old URI/range/cursor identities fail under their own documented boundaries. Zero TTL is not asserted to delete already-delivered content from a host.

### C. Cancellation, concurrency and lifetime tests

- [ ] Unit/in-memory doubles with explicit barriers prove: cancelling one browser wait leaves another request usable; tokens do not cross requests; video/retained-publication cleanup finishes; no duplicate dispatch; cancellation after mutation does not become a fabricated safe-retry failure.
- [ ] Drive those doubles through the same new `McpService` lifecycle method used by stdio, not solely `ServerHandler::serve`. Cover EOF with work in flight, signal/startup cancellation, slow cleanup past the SDK's short drain, output-write failure/backpressure, and an execution waiter dropped while cleanup remains owned. Inject short application deadlines and assert honest incomplete cleanup.
- [ ] Concurrent start/start, start/stop, stop/start, shutdown/delayed-connect and candidate-status failure: one lifecycle owner, no late installed session, no lost stop task or leaked candidate, no active slot prematurely reusable during stop. Preserve ended-session recovery behavior.
- [ ] Actual-binary tests pipeline independent request IDs and receive correctly associated results without requiring response order; include concurrent catalogue/status with cancellable work. Never use the SDK's default client cache as proof of actual wire concurrency.
- [ ] Local real-browser rows cancel an in-flight wait, then use the same browser successfully; close stdin with a managed session active and assert browser/descendant process termination and profile-lock release within the declared bound. Repeat with an attached disposable externally owned Chrome: Krometrail exits/detaches but Chrome remains alive, then the harness explicitly closes it. Supervise all processes and clean temporary roots even on assertion failure.
- [ ] When qualified FFmpeg is available, cancel video encoding/publication and close stdin during work; assert encoder termination/reaping and no published partial artifact/staging leak. The deterministic cleanup test remains mandatory without FFmpeg; optional encoder absence is reported, never a false live pass.

### D. Additional-friction regressions

- [ ] Stop/profiles omitted or empty args work; any unsupported argument fails before port access, especially a mistaken stop selector. Schema and runtime agree.
- [ ] Start/attach/status share concise/expanded/full projection semantics and generated response preferences; invalid preferences fail before connecting. Defaults preserve essential identity and diagnostics, full preserves the acquired canonical status, no images are invented.
- [ ] Unknown route/method sentinels never appear in default diagnostics, protocol error strings or logged spans; canonical registered names and local correlation IDs remain. Do not echo client metadata or enable SDK request Debug logging.
- [ ] Description/annotation tests establish current lifetime/offline-read guidance, closed-world local inventory/retained operations, open-world browser/current-geometry work, and truthful destructive/idempotent hints.

### E. Concrete client and release qualification

- [ ] Add a reusable raw JSON-line process harness with **bounded reads, independently drained stderr, deadlines, child kill/wait on failure, and temporary-state cleanup**. Support an explicit test-only executable/command override so the same probes run against built, downloaded and plugin-launched binaries; never expose it as a product CLI command. Caller-specified command vectors are test configuration, not shell strings.
- [ ] Use a current official rmcp **Discover** client and an **Initialize** client against the actual executable, not two in-memory halves sharing synthesized info. For modern error rows disable stale-on-error and cache. Raw-wire assertions remain independent of SDK decoding. Run the actual Orogen discovery client/probe if available and report its exact version; otherwise explicitly leave Orogen-specific retest unqualified rather than claim the original private receipts were reproduced.
- [ ] In fresh **Claude Code and Codex** supported-host sessions, use the candidate plugin/explicit candidate binary with temporary browser state and a deterministic local fixture: list every tool page, start, list pages, navigate/interact, inspect structured target/interaction IDs, receive a real bounded screenshot, resolve a range, generate a still artifact and retrieve its canonical image/manifest through a client that exposes resources/read. Native host tool/image/structured delivery must be observed, not inferred from a terminal rendering or raw JSON file. Record host versions and which host actually supports/executed resource reads; the modern SDK client's resource qualification does not imply a native host exposes that UI.
- [ ] Exercise `inline_images:false` and omitted/true images, concise/full detail, one degraded/failure result and temporal resource links. Validate decoded image format/dimensions and manifest/source identity; fixtures containing fake PNG headers prove only transport plumbing, not image delivery.
- [ ] Record binary path/hash/version, plugin projection/version/launcher target, client version and lifecycle, **observed negotiated/requested protocol**, OS/browser version, capability snapshot, catalogue page/byte metrics and per-operation outcomes. Keep receipts local or as CI artifacts, metadata-only/synthetic-fixture-safe; no user pages, credentials, arbitrary client metadata or raw base64 in committed work evidence.
- [ ] Fresh installed Pi gateway may be qualified as an additional concrete integration, but it is not a substitute for native Claude/Codex. Check the installed structured-result adapter version/bytes against its accepted external source fix. Do not turn that known outside-package issue into Krometrail JSON duplication or a silent package edit.
- [ ] Required Linux and macOS live rows must produce receipts bound to the exact candidate SHA. Existing CDP macOS capture receipts alone are not MCP interoperability evidence. Preserve browser opt-in gates: default tests do not depend on Chrome, explicitly required release qualification must fail rather than skip when prerequisites/receipts are missing.

**Release sequence:**

1. Implement in the parent context, updating this item's findings as facts change. Run focused tests after each boundary, then the full Rust formatting, wire-enum, locked check/test, Clippy and distribution gates. Put `~/.cargo/bin` first and record Rust/Cargo identities. Run locked Rust 1.88 check/tests for the changed dependency graph; clean every custom target/worktree created, not the pre-existing shared target. Build docs through the generator.
2. Recommend **Krometrail 1.7.0**: additive current-protocol support and bounded discovery/presentation improvements are meaningful minor functionality. No public tool/domain rename or unsupported broad architectural break justifies 2.0. No temporal-vision release is warranted unless its source actually changes (none is planned).
3. Prepare release projections using the repository helper's `--dry-run`/`--prepare` workflow as verified by parent, so the **one comprehensive independent Astra reviewer** sees integrated code, tests, docs, plugin/version projections, remaining evidence and release changes. No release tag/publication before that review. Fix accepted findings and rerun affected plus aggregate gates; the reviewer is not a second implementer fanout.
4. Commit/push the reviewed candidate, run exact-SHA Linux/macOS qualification, and bind the final report to the release commit (repeat relevant gates if runtime changes). Use the existing Cargo-authoritative release helper for the final version/tag/push transaction; verify the immutable tag points to reviewed code. Publish via the current release workflow, never `latest` polling from the plugin. Update any external skills/catalog projection through its established owning repository, if that publication is part of the parent release flow.
5. Strengthen release artifact smoke from `--version` alone to modern discovery + legacy initialization + complete paginated listing + bounded EOF on each runnable platform asset **before upload/publication**. The test harness may launch a native asset or an explicit container/QEMU command vector for Linux arm64; it must probe the staged asset, not accidentally `target/debug/krometrail`. Keep actual browser qualification on native Linux/macOS. Windows remains accurately labeled best-effort rather than a new supported-browser promise.
6. After publication, download exact-version Linux/macOS assets where runnable, verify official checksums/attestations and version, and rerun the metadata probe against the downloaded executable. In a disposable managed root, invoke the released plugin launcher, verify it selects **that exact release**, then perform the live supported-client smoke. Check plugin manifests/catalog marker all project the same Cargo release. Do not mutate unrelated standalone installs or the user's default browser profile.
7. Installing/updating a package is not reloading an already running MCP process or refreshing a host's tool catalogue. Start explicit fresh qualification clients and capture the actual server identity. Report publication, installation and fresh-session qualification separately; if a user's existing session requires restart, say so. If either native host or required platform cannot be exercised, report that external blocker and leave the corresponding release qualification incomplete rather than invent a pass.

## Design limitations and handoff notes

- This is a source-grounded design with one isolated **old-binary metadata measurement**, not a completed upgrade or a modern protocol qualification. Parent owns implementation, execution evidence and the final review/release sequence.
- The 3.2 tag's sequential opening-request behavior and short response drain are verified library facts; whether a particular candidate cleanup path leaks is not yet a newly reproduced Krometrail bug. Implement/qualify D5 rather than attributing unobserved leaks to the SDK.
- Stable catalogue caching only reduces repeated fetches; a host that injects all 51/52 descriptors into model context still pays aggregate schema cost. Pagination is not a promise to satisfy a 256 KiB **aggregate** client ceiling. That separate Orogen limit remains a client concern.
- Existing video live tests that seed the old flat data root conflict with the current per-instance cache lifetime. Repair an exercised stale fixture rather than count its skip as coverage or weaken instance isolation. Prefer obtaining retained frames through the live candidate process.
- Broader browser/CDP bugs and pre-existing reliability backlog do not become this feature's implementation scope merely because live qualification touches them. Record genuine newly exposed product bugs under the existing test-integrity rules, distinguish stale tests from product faults, and do not weaken tests to release.


## Implementation discoveries

- The actual-binary `shutdown_is_bounded_when_stdout_is_not_consumed` regression reproduced a process still alive 35 seconds after SIGTERM with stdout held unread (job 11). This is the anticipated D5 output-worker lifetime defect, not a browser failure. Tokio runtime destruction waits for a blocking stdout write; the narrow composition-root correction uses bounded runtime shutdown after application cleanup has completed or explicitly failed. The regression remains mandatory.

Documentation brief: existing integration users and contributors; existing docs/ reference and foundation venues; purpose is connecting and correctly traversing the modern MCP surface. Retain catalogue/reference structure with a scope-first opening and plain tech-doc style (short, literal, no marketing). Reader path: supported versions → all-page discovery → cache/process lifetime → evidence reads → failure recovery. Must preserve local-only transport, one registry, evidence/privacy and ownership distinctions; no migration essay or new CLI surface.

- Refinement: synchronized post-discovery backpressure also exposed an unbounded SDK transport-close wait before runtime teardown. The adapter now bounds transport drain separately (three seconds, inside the shared application deadline), reports interrupted unread responses as a non-clean transport exit, and still drains owned application cleanup. The test first observes discovery to avoid racing signal installation under load; unread truncated JSON is not asserted delivered.

### Settled catalogue correction (supersedes all-version paging in D3/D4 and B/E)

Native Codex 0.153.4 negotiated 2025-06-18 and issued only one tools/list; it ignored nextCursor and exposed only eight tools, excluding startup and temporal operations. Evidence: /tmp/krometrail-host-qualification/codex-wire.jsonl and codex-final.txt. This is a supported-host regression, not a hypothetical consumer. Follow-up Astra xhigh design confirms: both legacy versions return the complete same immutable sorted catalogue without continuation/cache fields; any legacy cursor is rejected. Modern 2026-07-28 retains 8-tool/192-KiB pages. Choose by SDK-resolved per-request version; no client-name exceptions or second schema authority. Legacy aggregate overhead remains explicit. Modern oversize-descriptor failure must not block legacy listing. Final independent comprehensive review remains required.

Full stable and minimum-Rust suites reached the existing temporal-evaluation contract assertion requiring generated docs to have no unstaged diff. Documentation generation intentionally changes that file; stage the generated projection before re-running rather than altering the test or discarding current docs.

Linter qualification correction: use exact Clippy 1.98.0 in CI, release helper and contributor commands, retaining workspace/all-targets/-D warnings with only clippy::chunks_exact_to_as_chunks exempted (13 pre-existing style findings in untouched independently-versioned temporal-vision). This preserves newer correctness lints instead of pinning to 1.95. Distribution assertions pin the identical invocation. No temporal-vision source/version change.

### Integrated local qualification update

- Modern/legacy raw executable matrix and official SDK tests pass after catalogue correction. Mixed-version requests do not mutate the negotiated default. Modern cursor is rejected on legacy listing; both legacy versions and fallback return the same complete inventory in one response. Modern remains seven pages / 51 tools in the still-only configuration.
- Real Chrome Linux tests pass for actual screenshot and temporal-artifact decoding, retained reads after stop, cancelled wait isolation, managed EOF cleanup, and externally attached Chrome surviving MCP EOF. The receipts still identify the dirty pre-release candidate, not a published or exact-final revision.
- Native Codex 0.153.4 fresh isolated run passed after user explicitly approved unrestricted smoke execution (shell tool disabled; no user config changed). Observed initialization: 2025-06-18. One catalogue response contained all 51 tools and no nextCursor. Model received inline screenshots and structured IDs, saw MCP 427 on dark teal, clicked Confirm and observed Confirmed 913, generated a temporal bundle, successfully read its PNG through the native resource reader, saw image suppression produce text-only content, and stopped cleanly. Browser: Chrome 151.0.7922.137. Runtime candidate still reports 1.6.3. Evidence: /tmp/krometrail-host-qualification/codex3-{wire,session}.jsonl and codex3-final.txt; earlier failed discovery/approval receipts preserved separately.
- Native Claude Code 2.1.261 remains externally blocked: OAuth session expired and could not refresh. User has been asked to run claude auth login. No native Claude image-delivery pass claimed.
- Exact linter must invoke rustup run 1.98.0 cargo-clippy clippy directly: cargo's subcommand search can select an older PATH cargo-clippy even when cargo itself comes from rustup. The fixture reproduced 0.1.96 under cargo delegation; direct executable selection reports 0.1.98.

- User explicitly waived native Claude qualification for this release after the expired-login failure was disclosed. Native Claude image/resource delivery is unverified, not failed product behavior. Codex native qualification remains required and passed locally.
- Local release preparation completed at 1.7.0 with full locked gates. Stable and Rust1.88 full suites each passed 1,386 tests (17 opt-in ignored). Custom minimum-Rust target was removed after qualification.
- Exact candidate 46eec69e3848ed684f60a29709611ccabd6876c7: hosted Linux live qualification passed; macOS managed-browser screenshot/artifact/cancellation/EOF passed, but external-attach setup failed with browser_launch_failed immediately after DevToolsActivePort appeared. Run34059396247. Hypothesis: the port file preceded HTTP discovery readiness; bounded fixture preflight now probes actual discovery and preserves a concrete readiness error on failure. Do not claim macOS pass until rerun.
- EOF-only unread stdout probe (no signal) exited non-clean after3.17s with explicit interrupted-transport error, after discovery succeeded. Added a separate regression alongside SIGTERM backpressure.

- Hosted candidate Rust quality and minimum-Rust jobs passed. Distribution job failed solely because its separate runner lacked the exact1.98.0 release-helper linter (the rust job's installation is not shared). Added explicit installation to that job and a contract assertion.
- Real package-owned launcher qualified against staged local release binary1.7.0 with SHA256969277f638b7d8da1b29ce45133595377c13ba550b6d933ce2ec606a032e6c6b; no standalone installation changed. Modern catalogue:51tools,7pages,max168295bytes. Each legacy catalogue:51tools,1page,844710bytes. Actual browser evidence, cancellation and managed/attached ownership tests passed through the launcher. This tests a pre-staged candidate, not published-asset download or native plugin installation/reload.
