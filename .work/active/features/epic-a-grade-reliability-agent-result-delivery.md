---
id: epic-a-grade-reliability-agent-result-delivery
kind: feature
stage: drafting
tags: [agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Preserve essential results in the agent-visible MCP response

## Outcome and priority

Text content contains only a success summary while usable result data is separate structured content. A client exposing text alone can hide page IDs and other recovery authorities. A recent local agent report observed exactly a success-only list_pages result, but the server wire response and integration renderer were not captured together.

- **Priority:** P1 — wave 1 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced response behavior; the cause of the reported integration incident is not yet established.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized for the bounded checkpoint/design below after the user asked to continue execution. No release or paid model-effectiveness qualification is authorized.

## Evidence

- crates/krometrail-mcp/src/response.rs:583 — successful text summary
- crates/krometrail-mcp/src/response.rs:989–1041 — text content versus structuredContent
- crates/krometrail-mcp/src/response.rs:1158 — ListPages result projection

## Acceptance criteria

- [ ] Capture a privacy-safe comparison of server wire result, client-decoded result, and model-visible result for list_pages, browser_status, inspect_page, temporal range resolution, and representative failures. Record plugin, binary, client, and protocol versions.
- [ ] Every currently supported integration exposes the essential identifiers, outcomes, recovery guidance, and requested observations needed for the next action; a bare success line is not sufficient for a data-returning tool.
- [ ] Add regression coverage through the actual integration delivery boundary, not only assertions against structuredContent inside Rust.
- [ ] Preserve bounded output, omission reporting, image/resource delivery, and privacy. Do not dump unlimited JSON into text or create compatibility paths for hypothetical clients.

## Implementation direction and boundaries

Fix supported-client delivery or supply a bounded useful text projection according to observed integration behavior. Keep one canonical result authority; the rendering strategy is not settled by this backlog item.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-mcp-locator-ergonomics` — related authority/context, not an implicit blocking dependency.

## Authorized execution checkpoint — 2026-09-05

The user asked to continue the reliability plan after the accepted Flash pilot. This item starts with an Astra-medium investigation/design checkpoint: trace the canonical result through supported integration delivery, identify the concrete consumer that loses essential data, and propose the smallest bounded current-contract correction with a reproducer. Preserve action outcomes, resource/image delivery, privacy, and omission accounting. Do not add compatibility text for hypothetical consumers or implement a broad projection redesign before the delivery boundary is established. Record evidence, affected file ownership, and a focused verification plan here for parent adjudication.

## Diagnosis checkpoint — 2026-09-05

**Disposition: remain drafting; no production correction implemented.** Inspection base is `ba69f4040ff77c47635016e8706c67e99c0831a5` in the isolated `work/result-delivery` worktree. The loss is reproducible in an installed, concrete integration adapter, not merely in an imagined text-only client. This establishes a current delivery defect, not the complete causal history of the reported screenshot incident.

### Supported contract and installed identities

The product installation guide names native Claude Code and Codex plugins; the manual MCP guide also permits a standalone stdio client. The shipped plugin supplies a launcher, MCP declaration, and skill, **not a result renderer**. Pi is a concrete locally deployed consumer of that same plugin through Nathan's Pi plugin host. It is not named as a native host in Krometrail's public installation guide; this distinction must survive any public support claim.

Observed locally without connecting or changing a browser session:

| Layer | Evidence |
| --- | --- |
| Checkout | Cargo root/workspace version `1.6.2`; pinned Rust MCP SDK `rmcp 0.11.0` |
| Installed Krometrail plugin | `~/.pi/agent/plugin-host/plugins/nklisch-skills/krometrail/.claude-plugin/plugin.json`: `1.6.2`; `.mcp.json` executes its `bin/krometrail mcp` with the managed-root environment |
| Actual named gateway configuration | `~/.config/mcp/mcp.json` declares `krometrail-browser` using that installed plugin launcher, arguments `["mcp"]`; only command/argument fields were extracted, not secrets or unrelated configuration |
| Standalone binary | `/home/nathan/.cargo/bin/krometrail --version` returned `krometrail 1.6.2`; this does **not** prove which managed binary the incident ran |
| Pi plugin host | installed `@nklisch/pi-plugins 0.8.2` under `@nklisch/pi-enhanced/node_modules`; `dist/pi/extension.js` calls `createMcpAdapter({ configOverlay: config })(pi)` |
| Pi adapter | installed `@nklisch/pi-mcp-adapter 2.21.0-nklisch.2`; production compiled files are locally available |
| Pi client | installed `@earendil-works/pi-coding-agent 0.85.1`; Node `v24.17.0` |
| MCP JS SDK | adapter declares `@modelcontextprotocol/client 2.0.0` and `@modelcontextprotocol/core 2.0.0`; diagnostic imports the installed core result decoder |
| Session/protocol | MCP status returned zero connected servers, cached `krometrail-browser` with 52 tools and unconnected `krometrail`. No initialization handshake or browser was started. Negotiated protocol, actual managed executable hash/version, browser version, and incident-time client/plugin versions are unavailable from this checkpoint. Do not substitute source defaults for negotiated protocol evidence. |
| Claude Code/Codex delivery | No native host result capture was made. Their delivery behavior is unqualified here; do not claim either loses structured results. |

Pi extension and SDK documentation were read in full, with the relevant session-format reference and extension tool example. The documented tool contract distinguishes `content` (model input) from `details` (rendering/state). Outside packages and repositories remained read-only.

### Concrete trace

1. `crates/krometrail-mcp/src/response.rs:572–595`: `map_operation_result_with_novelty` first maps the canonical operation, then projects its requested detail. Successful summary text is only `"{tool} succeeded"`. Degraded and failed outcomes have separate summaries.
2. `response.rs:1158`: `ListPages` serializes its canonical `Vec<PageStatus>` result. `krometrail-core/src/browser/control.rs:63–69` carries `target`, `selected`, and `open_dialog`; `target` is a `SupervisedTarget`, not a bare ID. An ID is not missing from acquisition merely because it is missing from the summary.
3. `response.rs:989–1041`: `into_call_tool_result` applies capture health, emits summary text, native images, and resource links, then independently serializes `ToolResponse` to `structuredContent`. It deliberately avoids rmcp constructors that duplicate complete JSON in text. The envelope retains `result`, `status`, `interaction`, `warnings`, `images`, `resources`, `error`, optional `range_handle`, and optional diagnostics.
4. Installed adapter `tool-registrar.ts:49–60` / `dist/tool-registrar.js:45–57`: `resolveMcpResultContent` returns transformed content immediately whenever **any** content block exists. Structured content is only used when content is empty. Therefore a success line, image, or resource link prevents all structured-only facts from reaching this model-content projection.
5. Installed adapter `proxy-modes.ts:1197–1202`: ordinary successful gateway calls use that resolver and `guardMcpOutput`; the original decoded result is kept only in `details.mcpResult`. `mcp-output-guard.ts:258–295` keeps this raw result only below the 16 KiB details budget, otherwise stores it in a temporary file and substitutes a summary in details. Neither path promotes its structured facts to model-facing content. The 50 KiB/2000-line text guard is downstream of the loss and cannot recover it.
6. `direct-tools.ts:508–532` uses the same resolver; its guard call does not retain a raw result in details. `programmatic-extension.ts:399–422` also uses the resolver and retains raw details. These are code-traced paths, not separate live-host qualifications.
7. Error paths in proxy/direct tools transform `result.content` directly instead of using the successful resolver. Fixing only the success helper would leave error-only structured context behind. Krometrail's **existing** `failed_summary` (`response.rs:886–899`) already includes error code, message, recovery, and retry advice; do not incorrectly report those as universally missing. Structured context IDs, diagnostics, and batch step evidence can still be lost.
8. `mcp-code.ts:144–165`: scripting calls reuse `executeCall` but successful `tools.call` returns `details.mcpResult` when available. Explicitly emitting selected `data.structuredContent` fields could recover small successful results. This is a bounded diagnostic escape, not normal gateway delivery and not reliable for oversized raw results, where details contain a spill summary. Error envelopes discard that successful-data route. No such script was run against a live session here.

**Classification:** canonical server result-loss is not demonstrated. The server's summary-only text is insufficient by itself, but its structured payload is present by code trace. Adapter presentation loss is confirmed for nonempty content plus structured content. The reported selected-page/screenshot failure remains separate: no contemporaneous request, wire, decoded result, and provider payload were captured together, so neither wrong selection nor freshness nor successful launch can be inferred from this transport reproduction.

### Reproducer and confidence

`tests/fixtures/agent-result-delivery.mjs` is a diagnostic that imports the **installed compiled adapter** result projector and output guard, plus the installed MCP result decoder. It does not reimplement their logic. It JSON-round-trips six small synthetic wire envelopes, decodes them, and captures the decoded versus model-facing shape. The cases represent `list_pages`, `browser_status`, `inspect_page`, `resolve_temporal_range`, a failure diagnostic, and a degraded interaction. Field sentinels are intentionally minimal, not complete Rust domain fixtures.

Observed for every case: the decoder and guarded raw result retained the sentinel; the model-facing content contained only the supplied summary (plus the error prefix for the error case). An empty-content control exposed structured content; an image/resource-link control preserved those content types while suppressing the structured-only sentinel. No guard truncation occurred. This is a loss **before** size pressure, not a 50 KiB overflow explanation.

Representative sanitized shape:

```text
synthetic wire result: content=[text("list_pages succeeded")],
  structuredContent={tool:"list_pages",status:"succeeded",result:{target_id:"fixture-page-id"}},isError=false
SDK-decoded result: same fields and sentinel
adapter model-facing content: [text("list_pages succeeded")]
adapter raw details: same decoded result, not model content
```

Confidence is high for the installed helper seam and static call-site diagnosis; medium for attribution to the earlier incident because its exact installed/runtime identities were not captured. This is **not** a live Rust-server wire capture, full gateway execution, provider serialization capture, or model-effectiveness test. The fixture reports diagnostic confirmation, never a passing delivery acceptance claim. Its failure sample intentionally tests a structured-only diagnostic identity; actual Krometrail failure text is richer than the synthetic sample.

## Proposed bounded correction for parent adjudication

### Recommendation: repair the host delivery adapter first

The concrete defect is generic MCP-to-Pi presentation, so prefer repairing `pi-mcp-adapter` rather than teaching Krometrail to compensate for every host. Preserve Krometrail's `ToolResponse` as the sole semantic authority; the adapter performs transport presentation only and must not infer browser selection, action success, or freshness. Do not change Krometrail acquisition, retention, or result semantics in this item.

Replace the adapter's **content-present means structured-unnecessary** rule with a single bounded result-delivery routine shared by proxy, direct, programmatic, and error paths:

- Preserve existing summary text and native image/resource-link delivery, and expose distinct structured facts from the decoded result even when content is nonempty. Avoid repeating an identical already-serialized structured block; do not deduplicate unrelated human text by heuristic meaning.
- Feed structured presentation through a bounded encoder, not an unlimited `JSON.stringify` appended to text. Respect the existing total 50 KiB/2000-line model text budget across summaries and structured content. Use one representation of structured facts in model content, never both a full raw MCP envelope and its structured payload.
- For an over-budget structured result, expose an explicit omission marker **in model content**, the exact omitted byte/row accounting available to the renderer, and a read/offset-capable local spill authority for the complete decoded result. Reuse the adapter's existing output-spill ownership rather than a second browser cache or new Krometrail resource namespace. A path kept only in `details` does not satisfy delivery. Do not count a byte-prefix cut as an exact omitted-row count; report byte counts unless complete rows were counted.
- If spill publication fails, state that recovery authority is unavailable; preserve summary/error status and the bounded acquired preview. Never turn a dispatched action into a retriable failure because its presentation failed. No fake full-result guarantee.
- Under budget, model-visible structured facts must equal the decoded projected facts exactly. Under pressure, a complete-result spill reference and explicit omissions are mandatory. This guarantees recoverability, **not** that arbitrary oversized observations fit in one model message. No implicit switch to full detail, rerequest, browser mutation, or auto-read of linked resources.
- Raw details and spill files remain local evidence; do not include image base64 in text, error logs, or diagnostic transcripts. Native images remain separate. Avoid an extra raw-result spill when the existing one already holds the same authority. Do not add legacy request aliases or a Krometrail-specific schema to the generic adapter.

This is a bounded adapter design, not permission to edit the outside repository. The parent must authorize that ownership handoff before implementation. If the parent requires a Krometrail-only change, the alternative is a bounded essential-text view derived **after** final `ToolResponse` mapping in `response.rs`, not serialization of the full result and not a second acquisition path. That alternative needs explicit overflow/drill-down design for non-resource inventories before readiness; simply listing the first page IDs and saying “request full” is not enough when the same adapter also hides full structured results. Do not silently select this larger server projection design.

### Essential data to pin at the delivery seam

These are semantic assertions against current canonical objects, not proposed new wire aliases:

| Surface | Essential information |
| --- | --- |
| Every result | Tool and succeeded/degraded/failed status; warnings and error context; retry and recovery; diagnostic correlation ID and permitted diagnostic reference; explicit omissions and available drill-down authority |
| Start/status | Session ID, selected target ID or explicit absence, session state, page count, open-dialog target IDs/types; requested frame stride; capture failure/gap and retention-blocked state sufficient to distinguish current control from retained evidence |
| Page inventory | Exact target IDs from nested `PageStatus.target.target`, selected flag, lifecycle/visibility and dialog state; bounded title/URL discrimination already authorized by the canonical projection; exact omitted rows and access to remaining canonical rows if bounded |
| Inspect/snapshot | Selected/observed target, current URL/title and viewport when acquired; snapshot generation, exact returned actionable references with role/name/value/state, semantic outcomes, unavailable-part reasons, unchanged-generation and omission markers |
| Range resolution | Top-level range handle, session/target, original requested and resolved time bounds/anchor, capture quality/gaps/warnings, and omission counts for identifier enumerations; never imply retained coverage beyond the resolved evidence |
| Interaction/batch | Proven dispatch/outcome independent from observation and retained capture; exact interaction anchor and only observed timings, returned per-step outcomes and failing index, current observation and postconditions; never suggest replay solely because presentation degraded |
| Visual resources | Requested/primary image as native image when selected by existing preference; image role, identity/dimensions and canonical source/artifact/manifest resource references; absence of pixels with `inline_images:false` does not remove structured results/resources |

### Privacy and omission constraints

The adapter must project the already-authorized response, not query DOM, environment, profiles, event bodies, or resource contents to enrich it. Keep existing bounded/redacted URL and event semantics. Diagnostic paths already explicitly returned by Krometrail may be delivered to its operator; never add extra filesystem paths except the adapter-owned spill required for retrieval. No screenshot data or user-page text belongs in this item's committed evidence. Inline image suppression, detail selection, temporal ID caps, and canonical resource identities remain unchanged. Spill access is host-local response recovery, not a promise that underlying browser resources survive eviction, restart, or deletion.

### Ownership and verification required before acceptance

- **This branch:** only this feature item and `tests/fixtures/agent-result-delivery.mjs`; stage stays drafting.
- **Proposed adapter owner (separate repository/work item):** `tool-registrar.ts`, `mcp-output-guard.ts`, proxy/direct/programmatic result call sites, and their actual execution-boundary tests; scripting needs a regression to prevent returning a spill summary as if it were complete canonical data. No edits made there.
- **Krometrail follow-up owner:** contract-shaped deterministic server-result fixtures and a stdio-to-adapter qualification harness for these assertions. Production `response.rs` changes only if parent chooses the server alternative after review. No page-selection, worker sizing, or freshness edits.
- **Native-host qualification owner:** obtain Claude Code/Codex model-input delivery captures before claiming all documented native integrations pass. Pi qualification does not stand in for them.

Required tests after adjudication:

1. Drive deterministic real Krometrail test doubles through server mapping/stdio and the actual adapter tool execution, checking the model-content boundary for the five acceptance surfaces. Preserve an explicit wire/decoded/content comparison and versions. Use no paid model calls.
2. Both structured+summary and structured+image/resource content; empty structured-only control; existing text-only results; identical serialized structure without double delivery. Assert one canonical result authority and no extra resource reads.
3. Failed input versus proven-dispatch degraded result, batch failure, capture failure, diagnostics present, retry/recovery exact. Check error paths independently of success resolver.
4. Just below/at/above text and details limits, multibyte strings, large nested rows/identifier vectors, image blocks, explicit omissions and retrievable complete spill. Inject spill-write failure. Assert no silent omission and no base64 text expansion.
5. Concise/expanded/full and inline-images omitted/false/true preserve canonical IDs/outcomes; stale/evicted resource reads keep their existing failures. Do not change selection or freshness to make these pass.
6. Actual provider-input assembly or its deterministic SDK seam must prove content delivery without sending a request; renderer expansion in the terminal is not model visibility.

Tests run: `node /home/nathan/dev/.krometrail-result-delivery/tests/fixtures/agent-result-delivery.mjs /home/nathan/.pi/agent/npm/node_modules/@nklisch/pi-mcp-adapter` completed with six confirmed losses and two controls (no truncation). `git diff --check` is the scope/whitespace gate. No Rust build/test, browser qualification, native-client execution, provider request, paid run, or release gate was run. No browser/profile/store/build directory was created; shared `/storage/cargo-target` was untouched. The short diagnostic needs no build lock; future heavy qualification must use the shared `flock /tmp/krometrail-reliability-build.lock` background path.

**Parent decisions at this checkpoint:** accept adapter-loss attribution independently of the historical incident; authorize adapter-first ownership or explicitly choose the server alternative; commission real server-wire/provider-seam and native-host qualifications before advancing acceptance. This checkpoint does not close any unchecked acceptance criterion.

## Authorized adapter handoff — 2026-09-05

The parent independently reran the installed diagnostic after integrating this checkpoint (`c49a42f7`): six structured-fact losses confirmed; empty-content fallback and image/resource-link controls checked. The user then explicitly selected **Fix the Pi adapter** and identified `../pi-extensions` as the source repository. The source adapter is also `2.21.0-nklisch.2`.

Source repair is owned by `mcp-structured-result-delivery` in the Pi Extensions Workbench, isolated on branch `work/mcp-structured-result-delivery`. The original source checkout's unrelated lockfile/Ollama work stays untouched. Astra medium is evaluating the concrete shared delivery design before scoped Flash xhigh implementation and Astra review. The earlier suggested bounded routine remains a design candidate, not a mandate for a new serialization framework. Existing output/spill ownership should be reused wherever sufficient.

No installed extension, package version, production configuration, or browser session has been changed. Publishing/installing and native-host qualification remain separate; Krometrail acceptance remains open. The Pi Extensions ledger's pre-existing older-layout validation failures are recorded separately and do not constitute an adapter regression or authorize a Workbench migration.

## Adapter source correction accepted — 2026-09-05

Pi Extensions source repair is integrated on its main branch through `480058b`, with the completed source item at `.work/archive/mcp-structured-result-delivery.md`. Independent Astra review accepted the final correction. Parent-owned, status-preserving verification passed typecheck, 114 focused cases, full `npm run check` including 1,043 adapter tests, and packed-package qualification. The source fix preserves structured facts, explicit error/dispatch outcomes, native images, and recoverable bounded output; scripts use the acquired result rather than a display summary.

Review also caught and corrected lost schema/UI guidance under spill reuse, inaccurate JSON line-paging advice, recursive equality failures, a high-line-count argument-spread failure that misreported successful dispatch, and silent deduplication on comparison faults. Evidence now accurately distinguishes generic message delivery from actual offline Anthropic request construction through Pi AI 0.82.0, captured before dispatch and aborted without a network request. Parent reproduced the final three regressions before correcting them.

This closes the external **source repair**, not this item's complete supported-integration qualification. No adapter version was bumped, published, installed, or reloaded. The running installed adapter remains unchanged. Actual Krometrail server-wire-to-client qualification, installed Pi 0.85.1, and native Claude/Codex delivery remain unqualified; the historical selection incident is still not causally established. The original Pi Extensions lockfile and Ollama work were preserved unchanged. Its three pre-existing workflow-validator errors remain a separate limitation, with no migration performed.
