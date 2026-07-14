---
id: epic-temporal-debugging-workflow
kind: epic
stage: review
tags: [visual, storage, agent-ux]
parent: null
depends_on: [epic-temporal-vision-toolkit, epic-durable-browser-memory, epic-agent-browser-operation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-14
---

# Temporal Debugging Workflow

## Brief

This epic delivers Krometrail’s defining agent workflow: operate the browser normally, notice a symptom, inspect the interval around an interaction, receive compact temporal evidence, and progressively retrieve regions or source frames. It integrates retained ranges with the temporal visual crate and exposes the result through context-efficient MCP responses and resources.

Temporal queries resolve natural anchors once, display gaps and retention warnings, generate reproducible cached artifacts, and keep every summary traceable to source evidence. The default bundle combines a simple orientation view, change-aware storyboard, difference map, capture-quality summary, interaction markers, and source references.

This epic does not perform automatic root-cause diagnosis or deterministic replay. The agent remains responsible for interpreting the evidence and deciding what to inspect next.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, and Success
- `docs/SPEC.md` — Temporal Ranges, Temporal Queries, Regions of Interest, and Artifact Provenance
- `docs/ARCHITECTURE.md` — Temporal Range Resolution, Artifact Generation, MCP Boundary, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Temporal Debug Bundle and Progressive Detail

## Design decisions

- **Agent query surface:** Make one temporal debug-bundle tool the primary interaction/range entry point. Focused tools retrieve source frames, region filmstrips, individual artifacts, verbose event detail, and pin state for progressive drill-down rather than reproducing the bundle schema in every tool.
- **Implicit interaction range:** Resolve an unspecified interaction query from bounded pre-action context through the action lifecycle and post-action observation, with bounded trailing context. Every response reports the exact resolved range.
- **Default browser-event context:** Include a compact deterministic selection of errors, failed requests, navigation, and events nearest major visual changes. Request and response bodies and verbose event lists require drill-down.
- **Comparison scope:** Defer automatic comparison between interactions or sessions. The epic optimizes single-range investigation; agents can inspect two independently grounded bundles until comparison receives its own evidence-backed design.
- **Range authority:** Reuse `TemporalRangeResolver` and `ResolvedRange` as the only natural-anchor interpretation and resolved interval contract. The first child makes existing interaction/navigation/marker anchors durable; later children consume the exact result rather than resolving again.
- **Boundary ownership:** Keep temporal query and bundle policy in infrastructure-free application/domain services. `krometrail-store` implements frame, event, artifact, cache, and retention ports; the composition root adapts the browser-independent `temporal-vision` crate; MCP remains presentation and protocol translation.
- **Artifact authority:** Persist the exact `temporal-vision` image and provenance manifest through one cache/store authority keyed by source identities, artifact kind, parameters, and algorithm version. Do not introduce a Krometrail manifest copy or treat generated artifacts as authoritative source evidence.
- **Correlation posture:** Capture quality comes from retained frame metadata, declared gaps, retention state, and capture measurements. Browser-event proximity is deterministic correlation, not causal attribution; sensitive values and bodies remain absent by default.
- **Resource lifetime:** MCP resources expose only retained source frames and persisted artifacts through validated resource reads, never raw user-supplied paths. Eviction and session deletion invalidate resources explicitly; pinning protects the resolved source segments through the existing retention authority.
- **No human UI:** This is an agent/API/resource workflow with no traditional screen or journey. Mockups are intentionally skipped.
- **Dispatch:** Direct local mapping only. The autopilot caller prohibited nested agents and peer review, and the completed dependency evidence plus current core/store/temporal-vision/MCP seams resolved ownership and ordering without another discovery path.

## Decomposition

The epic is split by delivered investigation capability. Resolved temporal queries establish one durable interval authority first. Bounded artifact generation and recorded browser context then proceed as sibling consumers, after which the primary debug bundle joins their outputs while progressive evidence retrieval remains independently usable. MCP lands last to present both paths and qualify the complete local workflow. This keeps storage, image-processing, and protocol layers inside capability slices rather than making them child features of their own.

### Child features

- `epic-temporal-debugging-workflow-resolved-temporal-queries` — make explicit and natural anchors durably resolvable through the existing `ResolvedRange` authority — depends on: `[]`
- `epic-temporal-debugging-workflow-artifact-generation-and-cache` — adapt retained frames to temporal-vision under bounded work limits and persist reproducible cached artifacts — depends on: `[epic-temporal-debugging-workflow-resolved-temporal-queries]`
- `epic-temporal-debugging-workflow-capture-and-browser-event-context` — retain sanitized browser events and expose range-scoped capture-quality and event context — depends on: `[epic-temporal-debugging-workflow-resolved-temporal-queries]`
- `epic-temporal-debugging-workflow-temporal-debug-bundle` — compose the primary non-diagnostic visual bundle with capture warnings, markers, and bounded correlated events — depends on: `[epic-temporal-debugging-workflow-resolved-temporal-queries, epic-temporal-debugging-workflow-artifact-generation-and-cache, epic-temporal-debugging-workflow-capture-and-browser-event-context]`
- `epic-temporal-debugging-workflow-progressive-evidence-and-pinning` — deliver focused artifact, region, source-frame, and pin operations over the same retained evidence — depends on: `[epic-temporal-debugging-workflow-resolved-temporal-queries, epic-temporal-debugging-workflow-artifact-generation-and-cache]`
- `epic-temporal-debugging-workflow-mcp-investigation-surface` — expose tools/resources and qualify the end-to-end interaction-to-progressive-evidence workflow — depends on: `[epic-temporal-debugging-workflow-temporal-debug-bundle, epic-temporal-debugging-workflow-progressive-evidence-and-pinning]`

### Simplification arcs

- `epic-temporal-debugging-workflow-resolved-temporal-queries` — replace the store's deliberate missing interaction-anchor projection with durable reuse of the existing browser-operation contract; retain one resolver and one resolved-range type.
- `epic-temporal-debugging-workflow-artifact-generation-and-cache` — turn the existing artifact schema/retention hooks and root temporal-vision placeholder into one production adapter; remove any need for direct artifact rows, duplicate manifests, or a second frame reader.
- `epic-temporal-debugging-workflow-capture-and-browser-event-context` — extend the capability registry, generic timeline, and usage ledger with one sanitized event vocabulary instead of parallel CDP-event stores and counters.
- `epic-temporal-debugging-workflow-temporal-debug-bundle` — compose existing artifact and context authorities; do not reimplement visual selection, measurements, gaps, provenance, or event persistence in bundle code.
- `epic-temporal-debugging-workflow-progressive-evidence-and-pinning` — centralize focused reads and pin operations over `FrameSource`, artifact storage, structured geometry, and `RetentionStore`; MCP handlers remain thin.
- `epic-temporal-debugging-workflow-mcp-investigation-surface` — extend the current dynamic router, response patterns, stdio service, and capability registry rather than adding a temporal-only server, schema mirror, or raw-path resource surface.

### Decomposition risks

- The current resolver intentionally returns no durable interaction anchors. The first feature must connect browser-operation timing to storage without creating a competing interaction identity or allowing a returned action to outrun its queryable anchor.
- Encoded browser frames and temporal-vision's decoded, common-geometry input have different contracts. The adapter must make decode failures, visual epoch splits, gaps, and resource ceilings explicit while keeping image work off capture ingestion.
- Artifact files, SQLite metadata, cache keys, source-frame links, retention, and deletion must agree under failure. Publication cannot expose an artifact whose bytes or provenance are missing, and eviction cannot leave a reproducibility claim after a source frame disappears.
- Browser-event recording expands a default-enabled subsystem across CDP, storage, and query code. Redaction and bounded capture are boundary requirements; request/response bodies, credentials, and verbose traffic must not become default evidence through convenience.
- Artifact generation and browser-event context are semantically parallel but may both extend store migrations and root composition. Their feature designs should coordinate shared schema/composition write ownership rather than inventing a false dependency or duplicate adapters.
- A compact bundle can overstate confidence when frames are sparse, a range is partially retained, or browser events merely occur nearby. Capture-quality warnings and non-causal language are part of the required bundle contract, not optional presentation polish.
- MCP 0.11 currently has no resource implementation in this repository. The final feature must add readable, lifetime-checked resources without leaking filesystem paths or weakening the existing protocol-only stdout, cancellation, generated-schema, and stable-error guarantees.
- End-to-end scenarios must prove the integrated local API workflow without becoming the separate paid model evaluation owned by `epic-prove-temporal-advantage`.

## Implementation roll-up

All six child features are reviewed and `done`: resolved temporal queries; bounded artifact generation/cache; sanitized capture and browser-event context; the primary temporal debug bundle; progressive evidence and pinning; and the MCP investigation surface. The integrated runtime now resolves one natural anchor, generates/cache-validates traceable visual evidence, correlates bounded observed browser context, supports focused retained-evidence reads and pins, and presents the workflow through strict MCP 2025-06-18 tools/resources without paths or unsupported task/subscription semantics.

Every feature completed its Rust 1.85 locked format, workspace check/test, and Clippy-with-warnings-denied gates. Feature reviews repaired material findings including artifact waiter lost wakeups, browser-event collection recovery, bundle permit cancellation, unavailable-evidence wording, marker-anchor identity, and chronological event schema exactness. The epic advances to `review` for its required deeper aggregate pass; no epic approval has yet been performed.
