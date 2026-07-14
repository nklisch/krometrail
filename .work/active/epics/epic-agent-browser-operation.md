---
id: epic-agent-browser-operation
kind: epic
stage: done
tags: [browser, agent-ux]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-14
---

# Agent Browser Operation

## Brief

This epic gives coding agents the complete live browser workflow that temporal inspection extends. It delivers current structured page observations, generation-scoped actionable references, verified browser input, navigation and tab management, explicit waits, batches, page evaluation, and post-action screenshots.

The control surface follows contemporary browser-agent conventions while preserving Krometrail’s local-first and fail-fast posture. Every state-changing standalone action produces a live observation and an interaction anchor; stale references and silent no-ops become explicit failures rather than guessed successes.

This epic does not provide historical temporal bundles or derived visual artifacts. It establishes reliable ordinary browser use and the interaction records that later temporal queries reference.

## Foundation references

- `docs/VISION.md` — Product Thesis and Core Experience
- `docs/SPEC.md` — Current-State Observation, Structured Page Snapshots, Browser-Control Surface, and Capabilities
- `docs/ARCHITECTURE.md` — Structured Snapshots and References, Interaction Execution, Capability Registry, and MCP Boundary
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Design decisions

- **MCP tool shape:** Expose composable standalone lifecycle, observation, navigation, and interaction tools plus an ordered batch tool. Standalone and batch actions derive from the same action registry and generated contracts rather than maintaining parallel schemas.
- **Element targeting:** Treat snapshot-scoped accessibility references as the primary target form. Explicit CSS selectors and declared coordinate-space targets remain escape hatches for debugging and DOM-opaque surfaces, and responses identify their weaker validation guarantees.
- **Browser start default:** Launch an isolated reusable Krometrail-managed profile by default. Attaching to an existing debug-enabled Chrome, selecting another named profile, or requesting a temporary profile remains explicit.
- **Electron boundary:** Support explicit attachment to Electron renderer targets through a local remote-debugging endpoint using the same capability-probed CDP control surface. Electron's Node main process and Electron-specific native APIs remain out of scope.
- **Foundation reuse:** Treat launch/attach ownership, managed-profile defaults, renderer capability probing, Electron renderer classification, target supervision, and reconnect as completed inputs from `epic-rust-cdp-capture-foundation`. This epic extends that one browser session boundary instead of creating a second browser manager or Electron-specific control stack.
- **Control contract ownership:** Establish live observations, target selection, operation identities, and interaction records in the infrastructure-free core. CDP implements those ports, and MCP translates them; adapter types and protocol envelopes do not become public control contracts.
- **Current-state evidence:** Every state-changing standalone operation returns one explicit post-action live observation and interaction anchor. Observation or verification failure is a structured degraded result, never a guessed success; read-only inspection remains screenshot-free unless requested.
- **Persistence boundary:** This epic creates complete interaction records and emits timeline anchors through core ports. Durable indexing and retention remain owned by `epic-durable-browser-memory`, so browser control does not introduce a private in-memory or duplicate store.
- **Dispatch:** Direct local probes only. The caller prohibited subagents and peer review, and the foundation docs, completed CDP foundation, current core/CDP seams, and empty MCP adapter resolved the decomposition without another discovery path.
- **UI surface:** There is no traditional human screen or journey to mock. The agent experience is the typed MCP/browser-control API, so visual mockups are intentionally skipped.

## Decomposition

The epic is split by agent capability rather than implementation layer: first make the current page observable and references safely actionable, then build page lifecycle/navigation and rich interaction as sibling consumers of that shared observation boundary. Explicit waiting and batching compose both operation families, and the MCP feature exposes the integrated capability through generated contracts. Existing Chrome/Electron connection supervision is reused rather than re-scoped here.

### Child features

- `epic-agent-browser-operation-page-observation` — deliver structured snapshots, generation-scoped references, current-state inspection, screenshots, and the shared live-observation boundary — depends on: `[]`
- `epic-agent-browser-operation-browser-page-lifecycle` — expose browser start/attach/stop/status and page creation, selection, closure, navigation, reload, and history with post-operation evidence — depends on: `[epic-agent-browser-operation-page-observation]`
- `epic-agent-browser-operation-verified-interactions` — execute reference-first pointer, keyboard, form, scroll, drag, dialog, upload, and coordinate-fallback actions with explicit verification and interaction records — depends on: `[epic-agent-browser-operation-page-observation]`
- `epic-agent-browser-operation-waits-and-batches` — add explicit wait conditions and ordered batches that reuse standalone operation semantics, per-step outcomes, anchors, and final live observation — depends on: `[epic-agent-browser-operation-browser-page-lifecycle, epic-agent-browser-operation-verified-interactions]`
- `epic-agent-browser-operation-mcp-control-surface` — expose the complete control capability through capability-driven MCP tools, generated schemas, structured/image responses, and stable error responses — depends on: `[epic-agent-browser-operation-waits-and-batches]`

### Simplification arcs

- `epic-agent-browser-operation-page-observation` — replace the deferred snapshot/reference placeholders with one core-owned generation model and one resolver; do not retain parallel accessibility, DOM, and selector identity systems.
- `epic-agent-browser-operation-browser-page-lifecycle` — extend the existing supervised `BrowserSessionPort` and production connector instead of adding another browser process, target, reconnect, profile, or Electron adapter.
- `epic-agent-browser-operation-verified-interactions` — derive action variants, validation, execution routing, and interaction display from one registry rather than one bespoke handler contract per action.
- `epic-agent-browser-operation-waits-and-batches` — compose the standalone executor and completion policies; do not duplicate action implementations for batching or make network-idle waiting an implicit global policy.
- `epic-agent-browser-operation-mcp-control-surface` — grow the reserved MCP crate into a thin adapter and generate schemas from shared Rust contracts, eliminating handwritten schema mirrors and any alternate runtime surface.

### Decomposition risks

- Snapshot generations and backing-node validity are the least forgiving boundary: dynamic replacement, navigation, iframes, shadow DOM, overlays, and scrolling can make a visually plausible target stale. The observation feature must own invalidation and explicit refresh guidance before input work begins.
- Live observation couples screenshots, structured snapshots, action timing, and target continuity. Feature design must define honest partial-failure behavior without letting screenshot latency block the continuous recorder or allowing missing evidence to become success.
- Page lifecycle and interaction can proceed in parallel after observation, but both will extend shared operation/interaction contracts. Their feature designs must preserve one registry and avoid incompatible duplicate envelopes; the waits-and-batches dependency is the integration checkpoint.
- The interaction capability is broad enough to invite per-action architecture. Keep one executor shape with action-specific completion policies and split implementation into stories only when CDP mechanics differ materially.
- Browser control creates interaction records that durable memory later persists. Keep emission behind core ports and test with in-memory adapters so this epic neither blocks on storage nor invents a temporary production store.
- MCP lands last and can expose contract inconsistencies late. Generated schemas and thin handlers limit that risk; feature-level design must verify standalone and batch tools derive from the same registry rather than repairing divergence at the adapter.

## Aggregate implementation roll-up

All five child features are reviewed and complete: page observation, browser/page lifecycle, verified interactions, explicit waits and ordered batches, and the MCP control surface. The implemented boundary now forms one ordinary agent-browser workflow: the root runtime owns one controlled browser session, the core operation/capability registries define typed behavior once, CDP executes through the supervised exact-session path with request-aware cancellation, every state-changing operation returns honest current-state evidence and an interaction anchor, and MCP exposes 24 generated control tools plus four lifecycle tools over protocol-only stdio.

The aggregate implementation remains within the epic boundary: durable interaction persistence, temporal queries/artifacts, browser-event inspection tools, page/framework state, remote transports, replay, rollback, and cross-target batches are not claimed here. Rust 1.85, locked workspace gates, real Chrome control qualification, MCP protocol/binary qualification, and current runtime documentation are green.

## Aggregate review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none

**Accepted limitations**:
- Lifecycle requests serialize behind a browser connection attempt; this is deliberate for one-session ownership.
- `PressKeys` preserves command-chord text by contract; sensitive text belongs in `Fill`.
- Upload canonicalization follows symlinks under local operator authority; the former child-design wording overstated a root-containment guard, but no external contract or foundation document claims one.
- Network domains remain enabled after explicit network waits while operation-scoped subscriptions are dropped.
- Cross-layer MCP cancellation regression coverage remains parked in `idea-mcp-cancellation-protocol-regression`.

**Parked follow-ups**:
- `idea-upload-symlink-policy` records the sharp upload boundary for an explicit future product decision.
- `idea-fill-clear-dialog-race` records the lower-risk asymmetry between sequential fill clearing and eagerly-polled pointer gesture dispatch.

**Rejected / inapplicable**: adding a target-unavailable batch outcome, duplicate observation deadlines, disabling Network after waits, and `throwOnSideEffect` for the fixed coordinate hit-test had no new supporting evidence and remain correctly rejected.

**Evidence**: Independent cross-model aggregate review traced all five completed features and 25 stories through MCP, session supervision, the shared standalone/batch executor, reference invalidation, cancellation/shutdown, response mapping, redaction, and foundation documents. Rust 1.85 locked workspace check and Clippy passed; 418 workspace tests, 9 MCP tests, 5 binary smoke tests, and 8 real-Chrome capture qualifications passed with no relevant failures. Standard weight requires one pass only; no re-review was requested.
