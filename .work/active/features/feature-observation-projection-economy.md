---
id: feature-observation-projection-economy
kind: feature
stage: implementing
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Lean routine responses and viewport-anchored inspection that survives big pages

## Brief

Three observation-surface improvements from the 2026-07-19 shakedowns:

1. **Scroll/viewport geometry pass must degrade, not fail** (bug, found on
   Wikipedia at 8362 DOM-snapshot nodes): every `scroll` and `set_viewport` on a
   page above the 5000-node cap returns `status: degraded` with the entire
   post-action snapshot unavailable — the geometry decode in
   `control/snapshot.rs` (~line 846) hard-errors past `MAX_SNAPSHOT_NODES`,
   while the accessibility path (~line 514) handles the same pressure by
   omitting and reporting. A standalone `snapshot_page` on the same page
   succeeds. Fall back to the plain accessibility projection (no `document_rect`
   anchoring) with an explicit anchoring-omitted note — bounded-loss accounting,
   operation stays `succeeded`. Fix the query-oriented recovery text on a
   scroll while there.
2. **Trim the routine-success payload.** Concise action responses embed the full
   `record` block (sanitized echo of the caller's own parameters), three-stamp
   timing objects, and repeated context envelopes — a plain click is 2–4 KB and
   clicks/fills dominate call volume. Keep interaction id, outcome, and the
   observation in concise; move the record/provenance echo to expanded. Detail
   must not change action outcome (canonical-result-projection).
3. **Viewport-anchored explicit inspection.** Post-scroll viewport ranking
   exists only inside the scroll response. Expose an explicit anchor option on
   `snapshot_page` (and `query_page` if it falls out naturally) so an agent can
   ask "what is actionable on screen right now" directly — the natural question
   after any scroll, and the workaround for pages where the inline pass hits
   the node cap.

Absorbed backlog: `idea-scroll-geometry-node-cap`. Implementation via peeragent
Codex `gpt-5.6-luna` per operator decision (2026-07-19).

## Simplification opportunity

Unit 1's fallback may let the scroll-path geometry acquisition reuse the
accessibility acquisition's existing bounded-omission machinery instead of
carrying its own hard cap. Unit 2 removes payload rather than adding a new
detail tier if concise can simply shed the record echo.

## Explorer map (verified file:line)

- `MAX_SNAPSHOT_NODES = 5_000` — `crates/krometrail-cdp/src/control/snapshot.rs:15`.
  Hard error in `decode_dom_snapshot_with_geometry` at L846-854 (`malformed` →
  PageObservationFailed); accessibility decode instead accumulates omissions at
  the L1591 guard and reports at query time (L509-518).
- Geometry pass callers: `observe_after_operation_with_geometry`
  (`control/pages.rs:37-77`, `include_document_geometry=true`) used by
  scroll/set_viewport (`session/operations.rs:795`); plain
  `observe_after_operation` (pages.rs L26-35) elsewhere (operations.rs L883).
  `DOMSnapshot.captureSnapshot` sets `includeDOMRects` from the flag
  (snapshot.rs L332); layout bounds decode L935-972.
- `snapshot_page` never runs geometry: `PageControl::snapshot` snapshot.rs
  L234-251 (`include_document_geometry=false`), dispatch `control/mod.rs:180`.
- Viewport ranking lives in the MCP projection: `concise_snapshot`
  (`krometrail-mcp/src/response.rs:1449`), `expanded_snapshot` (L1478),
  `project_root_snapshot` `visual_viewport` arg (L1617-1640).
- `record` echo attached unconditionally at response.rs L984-987 inside the
  interaction arm (L967-992); detail projection (`project_response`
  L1596-1615) only rewrites snapshot_page/inspect_page bodies — record is
  never tier-gated today.
- `SnapshotPageRequest` comes from the `page_request!` macro
  (`krometrail-core/src/browser/observation.rs:1016-1034`, single `target`
  field); registry schema derives automatically from the wire type
  (`krometrail-mcp/src/registry.rs:161`), operation metadata
  `browser/operation.rs:288-289`.

## Design decisions

- **Geometry over-cap = omission, not error**: past the node cap the geometry
  decode returns the snapshot without layout bounds plus an explicit
  `geometry_omitted` marker; ranking falls back to the existing non-viewport
  action ranking. Operation status stays `succeeded` (bounded-loss accounting;
  pixels already cover the staleness need via the auto-image).
- **`record` moves to expanded**: concise keeps the `interaction` envelope
  (id, operation, timing) and `observation`; the `record` block (sanitized
  parameter echo) appears at expanded/full. Detail still never changes action
  outcome. SPEC response-tier wording rolls forward.
- **Anchor option on snapshot_page only**: `anchor: "document" | "viewport"`
  (default `document` = today's behavior); `query_page` is untouched — its
  semantic filters already narrow, and the economy goal doesn't justify a
  second surface.

## Implementation Units

### Unit 1: Geometry node-cap fallback
**File**: `crates/krometrail-cdp/src/control/snapshot.rs` (+ response
projection for the omission note)

- Replace the L846 hard error with geometry omission: decode DOM semantics
  without layout bounds, record `geometry_omitted: true` (or an omissions
  counter alongside the existing `presentation_targets`/`source_nodes`).
- Post-scroll projection renders targets with action ranking when geometry is
  absent and surfaces the omission explicitly.

**Acceptance Criteria**:
- [ ] Deterministic double with >5000-node DOMSnapshot table: scroll-path
      observation returns `succeeded`, snapshot available, explicit geometry
      omission, no `page_observation_failed` warning.
- [ ] Under-cap behavior unchanged (existing viewport-ranking tests pass).

### Unit 2: Tier-gate the record echo
**File**: `crates/krometrail-mcp/src/response.rs`, `docs/SPEC.md`

- Omit `record` from concise interaction results; include at expanded/full.

**Acceptance Criteria**:
- [ ] Concise click/fill/scroll results carry `interaction` + `observation`
      but no `record`; expanded carries `record`.
- [ ] SPEC response-tier section states the echo tier.

### Unit 3: Viewport-anchored snapshot_page
**Files**: `crates/krometrail-core/src/browser/observation.rs`,
`crates/krometrail-cdp/src/control/{mod.rs,snapshot.rs}`,
`crates/krometrail-mcp/src/response.rs`, `docs/SPEC.md`

- Break `SnapshotPageRequest` out of the macro; add optional `anchor` enum
  (default `document`). `viewport` runs the geometry acquisition (Unit 1
  bounded) and passes the visual viewport into the ranking projection.

**Acceptance Criteria**:
- [ ] `snapshot_page {"anchor":"viewport"}` after a scroll ranks in-viewport
      targets first (deterministic double with geometry rects).
- [ ] Default request shape unchanged; schema advertises the new field.

## Implementation Order
1. Unit 1 (fallback — Unit 3 depends on it)
2. Unit 2 (independent)
3. Unit 3

## Testing
- Regression: over-cap scroll (the Wikipedia failure) as a deterministic
  double; concise-vs-expanded record presence; viewport-anchored ranking.
- No new real-chrome tier needed.

## Risks
- Removing `record` from concise is a response-shape change; grep tests and
  plugin skill text for reliance on concise `record`.

## Implementation Notes

- Execution capability: inline implementation; the three units were cohesive
  across the core, CDP, MCP, and specification surfaces. Review weight:
  standard.
- Unit 1 delivered in `crates/krometrail-cdp/src/control/snapshot.rs` and the
  canonical snapshot/response projections: DOMSnapshot tables over the node
  cap now omit geometry with explicit `geometry_omitted` accounting while
  retaining the snapshot and successful operation outcome. Added a
  deterministic over-cap regression using a DOMSnapshot table with more than
  5,000 nodes.
- Unit 2 delivered in `crates/krometrail-mcp/src/response.rs` and
  `docs/SPEC.md`: concise interaction projections retain the interaction and
  observation but omit `record`; expanded and full retain the sanitized record
  echo. Added a projection regression covering all three tiers.
- Unit 3 delivered in the core request type, CDP snapshot control, and MCP
  projection: `snapshot_page` now accepts `anchor` with document as the
  default and viewport as the geometry-backed option. Added schema and
  deterministic viewport-ranking regressions.
- Verification: `cargo fmt --all -- --check`,
  `cargo check --workspace --all-targets --locked`,
  `cargo test --workspace --all-targets --locked`, and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` all passed.
