---
id: feature-response-evidence-economy
kind: feature
stage: implementing
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Make automatic evidence match what the action changed

## Brief

Refinements on top of `compact-live-observations` (done, 1.0.4) and
`agent-visual-response-surface-visual-defaults` (done), driven by a live shakedown
session. Four gaps remain between what an action returns and what an agent needs:

1. **Unchanged snapshots are re-dumped.** Every state-changing response — including pure
   selection/focus operations like `activate_page`, `select_page`, and `go_back` — embeds
   the full ranked target list (~45 targets on Hacker News) even when the snapshot
   generation did not change from the previous response. Per-target `states` arrays repeat
   near-universal defaults (`focusable: true` on every link). A single click costs
   thousands of tokens of mostly-identical data.
2. **Post-scroll evidence describes the wrong viewport.** After `scroll` to y=2000, the
   returned targets and semantic outcomes still describe the top of the page. An agent
   that scrolls to reveal content learns nothing from the structured response and must
   take a screenshot.
3. **No automatic image where structure is known-stale.** Strategic decision below.
4. **Full-page screenshots of tall pages are model-useless.** A 28,000px article returns
   one 1658x28276 image with no warning; downscaled for model input it is unreadable.

## Strategic decisions

- **Automatic image policy**: Staleness-triggered — keep routine operations image-off, but
  auto-inline one viewport image exactly when the structured projection is known-stale or
  low-information: after `scroll`, viewport changes, and activation. Chosen over
  image-on-every-action (token cost, redundancy) and over pure opt-in (agents stay blind
  after scroll). Preserves the cheapest-sufficient-evidence contract; explicit
  `inline_images` overrides always win.

## Simplification opportunity

Snapshot dedupe can reuse the existing generation identity: when the post-action snapshot
generation equals the previously projected one for the same target, project the identity
and omission counts instead of the target rows. Viewport anchoring can reuse the existing
ranking pass with the current visual viewport as the ranking window rather than adding a
second ranking system. Tall-screenshot handling should prefer guidance plus bounded output
(existing output-limit machinery) over a new tiling subsystem.

## Code map (explorer-verified, file:line)

- Presentation boundary: `crates/krometrail-mcp/src/response.rs` —
  `map_operation_result_with_capture` (:398) → `project_operation` (:830) →
  `project_live_observation` (:1060) → `concise_snapshot` (:1303) / `bounded_targets`
  (:1272, rank at :1382) / `semantic_outcomes` (:1110, budgets :29-30). All stateless;
  no cross-call memory exists today.
- Generation identity: `PageSnapshot.generation`
  (`crates/krometrail-core/src/browser/observation.rs:411`), assigned in
  `begin_snapshot` (`crates/krometrail-cdp/src/control/snapshot.rs:418-444`), which
  **reuses the same generation when document fingerprint + attachment are unchanged** —
  the dedupe key.
- States: `ExactTarget.states` clones `SnapshotNode.properties` (response.rs:1260;
  allowlist snapshot.rs:19-36). `focusable` is emitted on essentially every actionable
  node.
- Viewport: `PageState.viewport.visual_viewport` (observation.rs:214-221, :329) is in
  scope inside `project_live_observation` but not threaded into ranking/outcomes.
  `SnapshotNode` carries **no geometry**; boxes exist only on-demand via `DOM.getBoxModel`
  in the actionability resolver (snapshot.rs:1221-1253).
- Image defaults: `browser_inline_image_default` (`crates/krometrail-mcp/src/registry.rs:381`,
  applied :810-815; unit test registry.rs:1022). Post-action screenshots are already
  captured and merely filtered at response.rs:1417-1424 — auto-image is a default-policy
  change, not a capture change.
- Full-page screenshots: `crates/krometrail-cdp/src/control/screenshot.rs:41` (:103 for
  full-page clip); only byte limits (:24-25, hard errors :217-244); no dimension guidance.
- Session-scoped state home: `BrowserSessionOwner`
  (`crates/krometrail-mcp/src/session.rs:13`, already `Arc<Mutex<..>>`, reachable in
  `call_operation` via registry.rs:805,829-845).

## Design decisions

- **Dedupe mechanism**: track last-projected `(target_id, attachment_generation,
  snapshot_generation)` in `BrowserSessionOwner`; when the post-action snapshot's
  generation matches, concise/expanded projections emit a compact
  `{generation, unchanged: true, target_count, omissions}` marker instead of target rows.
  Explicit `snapshot_page`/`observe_live`/`query_page` are never deduped — they remain the
  drill-down authority (canonical-result-projection). State resets implicitly because
  generation changes on document/attachment change.
- **States pruning**: drop `focusable` from concise `states` only — presence in the
  actionable target index already implies it; expanded/full keep the complete property
  list. Chosen over an allowlist rework (proportional rigor).
- **Auto-image surfaces**: `Scroll`, `SetViewport`, `ActivatePage` flip default-on in
  `browser_inline_image_default`. `SelectPage` stays off (logical selection, no visual
  change); `GoBack`/`GoForward` stay off (navigation yields a fresh informative
  snapshot). Explicit `inline_images` always wins (existing `with_inline_default`).
- **Viewport anchoring scope**: only scroll and viewport-change observations get
  geometry — a single bounded `DOMSnapshot.captureSnapshot` layout pass attaches optional
  per-node document rects, and projection ranks/filters by visual-viewport intersection.
  Chosen over always-on geometry (cost on every action) and over per-candidate
  `DOM.getBoxModel` calls (N round trips, and candidates would already be page-top-biased
  before filtering).
- **Tall full-page policy**: keep capture semantics; add one warning with guidance when
  captured height exceeds 8192 px (suggest element/region capture or scrolling with
  viewport captures). No auto-downscale, no tiling subsystem.
- **SPEC roll-forward**: the "Routine operations remain pixel-light" sentence gains the
  staleness exception (scroll/viewport/activation embed one bounded viewport image by
  default); concise contract gains the unchanged-generation marker. Rolled forward at
  implementation time with the code change.

## Implementation Units

### Unit 1: Generation dedupe + states pruning
**Story**: `feature-response-evidence-economy-dedupe-projection`
**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/session.rs`,
`crates/krometrail-mcp/src/registry.rs`

- Add to session owner:
  `pub struct ProjectedSnapshotMemory { entries: HashMap<TargetId, (u64, SnapshotGeneration)> }`
  with `fn observe(&mut self, target: TargetId, attachment_generation: u64, generation: SnapshotGeneration) -> SnapshotNovelty`
  returning `Novel | Unchanged`.
- Thread a `SnapshotNovelty` (or the memory handle) into
  `map_operation_result_with_capture` from `call_operation`; only automatic post-action
  observations consult it (explicit inspection routes pass `Novel` unconditionally).
- In `concise_snapshot`/`expanded_snapshot`, `Unchanged` projects
  `{"generation": N, "unchanged": true, "target_count": M, "omissions": {...}}`.
- In `exact_target`, filter `focusable` from the cloned properties (concise arm only).

**Acceptance Criteria**:
- [x] Two consecutive actions on an unchanged document project the full index once, then
      the unchanged marker (projection test with `complex_snapshot()` fixture).
- [x] A navigation (new generation) re-emits the full index.
- [x] `snapshot_page` output is never deduped.
- [x] Concise `states` contain no `focusable` entries; expanded still do.

## Implementation Notes

- Unit 1 is complete: `BrowserSessionOwner` now tracks projected snapshot generations per target and attachment, while automatic post-action concise and expanded projections emit an unchanged-generation marker. Explicit snapshot inspection remains unconditionally novel.
- Concise target states omit redundant `focusable`; expanded and full projections retain it.
- Unit 1 verification: `cargo fmt --all -- --check`, `cargo check -p krometrail-mcp --all-targets --locked`, and `cargo test -p krometrail-mcp --all-targets --locked` passed after repairing the existing server projection assertion for the new marker.

### Unit 2: Staleness auto-image defaults + tall-page guidance
**Story**: `feature-response-evidence-economy-staleness-images`
**Files**: `crates/krometrail-mcp/src/registry.rs`,
`crates/krometrail-cdp/src/control/screenshot.rs`, `docs/SPEC.md`, skill instructions

- Extend `browser_inline_image_default` to return true for
  `Scroll | SetViewport | ActivatePage` (keep `TakeScreenshot | ObserveLive`).
- In `capture_screenshot`, when the decoded image height exceeds 8192 px, append a
  bounded warning (existing warning channel) with recovery guidance naming element/region
  targets and viewport captures.
- Roll forward SPEC.md Current-State Observation wording; update the plugin skill text in
  the same stride (coordinates with story-skill-inline-image-default-drift, which handles
  the already-shipped drift).

**Acceptance Criteria**:
- [x] `image_defaults_follow_operation_purpose_and_preserve_overrides` (registry.rs:1022)
      extended: scroll/set_viewport/activate_page default on; explicit false suppresses.
- [x] Full-page capture of a >8192px document succeeds and carries exactly one guidance
      warning; shorter documents carry none.
- [x] SPEC.md and skill text state the staleness exception.

- Unit 2 is complete: scroll, viewport-change, and page-activation operations default to one bounded inline image while explicit overrides remain authoritative. Decoded screenshots taller than 8192 pixels carry one bounded `ResourceLimitExceeded` guidance warning recommending element/region or scrolling viewport captures; the warning is projected through direct, live, and batch response paths.
- The current SPEC and plugin evidence instructions describe the staleness exception, and `docs/public/llms-full.txt` was regenerated with `bun run docs:build`.

### Unit 3: Viewport-anchored post-scroll evidence
**Story**: `feature-response-evidence-economy-viewport-anchoring`
**Depends on**: Unit 1 story (both rewrite the same projection functions)
**Files**: `crates/krometrail-cdp/src/control/snapshot.rs`,
`crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-mcp/src/response.rs`

- Add `SnapshotNode.document_rect: Option<CssRect>` (None everywhere except
  geometry-bearing snapshots; serde-skipped when None so existing projections are
  unchanged).
- For scroll and set-viewport observations only, `begin_snapshot` (or a sibling
  acquisition path) runs one `DOMSnapshot.captureSnapshot` layout pass and joins layout
  rects onto decoded AX nodes by backend node id — reuse the DOM-semantics acquisition
  machinery added by the frame-query story rather than a new client.
- `bounded_targets` and `semantic_outcomes` accept the visual viewport
  (`Option<&CssRect>`): when provided, targets intersecting the viewport rank ahead of
  the existing action ranking, and semantic outcomes prefer viewport-intersecting text.
  When geometry is absent, behavior is exactly today's (total fallback).

**Acceptance Criteria**:
- [x] After a scroll observation on a fixture where distinct targets sit above/inside the
      viewport, the concise index leads with in-viewport targets and semantic outcomes
      describe in-viewport text.
- [x] Non-scroll operations acquire no `DOMSnapshot` layout pass (command-recording
      double asserts absence, mirroring the frame-query story's test style).
- [x] Geometry-less snapshots project byte-identical to pre-change output.

### Unit 3 Implementation Notes

- Unit 3 is complete: scroll and set-viewport post-action observations request one bounded
  DOMSnapshot layout pass, join valid bounds to AX nodes by backend id, and use visual-viewport
  intersection to rank concise targets and semantic outcomes. Explicit and non-scroll snapshots
  keep the prior acquisition path.
- Geometry remains optional and omitted from serialized nodes when unavailable, preserving the
  prior geometry-less projection shape and ordering.
- Unit 3 verification: focused CDP/core/MCP tests passed; the known launcher discovery flake
  failed once and passed on its required single rerun.

## Implementation Order
1. Unit 1 (dedupe + pruning) — self-contained projection change.
2. Unit 2 (defaults + guidance) — independent, small.
3. Unit 3 (viewport anchoring) — builds on Unit 1's touched functions.

## Testing
- Projection tests in `response.rs mod tests` (fixtures exist: `complex_snapshot()`
  :2543) protect the concise contract; registry default test protects override semantics.
- Command-recording deterministic double protects "no extra CDP cost on routine actions"
  (the regression risk of Unit 3).
- No new real-chrome tier: geometry join is deterministic given recorded CDP fixtures.

## Risks
- **Dedupe hiding real change**: generation reuse keys on document fingerprint; dynamic
  pages that mutate without fingerprint change would project `unchanged` while pixels
  moved. Mitigated: the auto-image policy (Unit 2) covers scroll/viewport, and any
  explicit inspection bypasses dedupe. If fingerprint granularity proves too coarse in
  practice, the marker still names the generation so an agent can drill down.
- **DOMSnapshot join misses nodes** (AX node without backend-id match): fall back to
  geometry-less for those nodes — ranking degrades gracefully to today's order.
