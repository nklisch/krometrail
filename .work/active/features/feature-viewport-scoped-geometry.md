---
id: feature-viewport-scoped-geometry
kind: feature
stage: done
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Viewport-scoped geometry for large-page snapshots

## Brief

`snapshot_page {"anchor":"viewport"}` degrades to document-order ranking on any
page whose DOMSnapshot node table exceeds `MAX_SNAPSHOT_NODES` (5000), because
viewport ranking needs the geometry pass and such pages omit geometry
(`geometry_omitted: true`). Verified on Wikipedia "Web browser" (~8000 nodes,
scrolled to y=2600): the viewport-anchored snapshot returned the same
top-of-document nav items as document anchor, honestly flagged with
`geometry_omitted`. The degradation is correct and explicit — but the practical
consequence is that viewport anchoring only functions on small pages where it
is least needed, and is unavailable on exactly the large pages where "what is
actionable on screen right now" matters most.

Fix direction from the shakedown: keep geometry available for viewport ranking
on large pages by bounding the geometry that is actually used, not by refusing
the pass — e.g. acquire/retain DOMSnapshot rects only for nodes intersecting
the current visual viewport (a much smaller set) when the full-document node
count exceeds the cap, or allow the explicit `anchor: viewport` request to opt
into the larger geometry cost. The existing honest `geometry_omitted` signal
stays for whatever remains genuinely unavailable.

## Simplification opportunity

None identified beyond reusing the existing geometry plumbing; the change
should bound an existing pass rather than adding a parallel one.

Origin: `.work/backlog/idea-viewport-anchor-unusable-when-geometry-omitted.md`
(2026-07-19 third shakedown).

## Architectural choice

Bounded viewport-scoped decode in `krometrail-cdp`'s snapshot path. Key
observation: the DOMSnapshot response is already fully materialized as a
`serde_json::Value` before `MAX_SNAPSHOT_NODES` is consulted — the cap bounds
*retained decoded state*, not wire cost. So when the node table exceeds the cap
AND the caller anchored to the viewport, we can decode geometry (and semantic
metadata) for only the layout entries whose bounds intersect the current visual
viewport — a set that is physically bounded by screen area — instead of bailing
to `geometry_omitted: true`. Document-anchored over-cap behavior is unchanged.
Alternatives rejected: raising/making the cap configurable (unbounded retained
state, pushes the decision to the agent), and a second viewport-only CDP
capture call (extra round trip, still returns the whole document).

## Design decisions
- **Viewport rect acquisition order**: `snapshot()` fetches
  `Page.getLayoutMetrics` *before* `capture_snapshot` when
  `anchor == Viewport`, passes the visual-viewport `CssRect` (document
  coordinates, from `cssVisualViewport` pageX/pageY/width/height) down into the
  decode, and reuses it for `with_visual_viewport` afterward (drop the second
  fetch) — one fetch instead of two, and the decode gets the rect it needs.
- **Selection truncation**: if viewport-intersecting layout entries exceed
  `MAX_SNAPSHOT_NODES`, keep the first cap-many in layout order and set
  `geometry_omitted: true` (explicit loss signal per bounded-loss-accounting);
  otherwise `geometry_omitted: false`.
- **Semantic metadata**: decode node metadata (id/label/test-id attributes) for
  exactly the selected node indexes, so on-screen nodes regain labels on large
  pages. The full-document metadata map stays empty beyond the selection —
  acceptable because the AX tree remains the semantic backbone.

## Implementation Units

### Unit 1: Viewport-scoped DOMSnapshot decode (trickiest)
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
fn decode_dom_snapshot_with_geometry(
    response: &Value,
    document: &DocumentFingerprint,
    target_id: TargetId,
    include_document_geometry: bool,
    viewport_scope: Option<CssRect>,   // NEW: Some(_) when anchor == Viewport
) -> Result<DecodedDomSnapshot>
```

Over-cap branch (`backend_ids.len() > MAX_SNAPSHOT_NODES`):
- `viewport_scope: None` → current behavior (geometry request → omitted;
  otherwise limit error).
- `viewport_scope: Some(viewport)` → scan the layout table once; select
  entries whose bounds rect intersects `viewport`; cap the selection at
  `MAX_SNAPSHOT_NODES` (set `geometry_omitted` on truncation); build
  `document_rects` for the selection via `backend_ids[node_index]` (O(1) per
  entry, no full node decode); decode `SemanticNodeMetadata` for exactly the
  selected node indexes.

**Implementation Notes**:
- Bounds are `[x, y, width, height]` document CSS coordinates — same space as
  the visual viewport rect built from `cssVisualViewport` pageX/pageY.
- Intersection: standard half-open rect overlap; zero-area entries (hidden
  layout objects) fail intersection naturally.
- Under-cap pages: existing full decode path unchanged (viewport_scope unused).

**Acceptance Criteria**:
- [x] Deterministic double: 5001+-node DOMSnapshot + viewport rect → snapshot
      has `geometry_omitted: false`, document_rects only for intersecting
      nodes, and semantic metadata for those nodes.
- [x] Same fixture without viewport scope → `geometry_omitted: true` (existing
      behavior preserved; existing test keeps passing).
- [x] Truncation fixture (> cap intersecting entries) → capped rects and
      `geometry_omitted: true`.

### Unit 2: Single layout-metrics fetch feeding decode and response
**File**: `crates/krometrail-cdp/src/control/snapshot.rs` (`snapshot()`,
`capture_snapshot` signature)

Move the `Page.getLayoutMetrics` fetch ahead of `capture_snapshot` for
viewport-anchored requests; thread `Option<CssRect>` through
`capture_snapshot` → `decode_dom_snapshot_with_geometry`; attach
`with_visual_viewport` from the already-fetched rect.

**Acceptance Criteria**:
- [x] Viewport-anchored snapshot issues exactly one `Page.getLayoutMetrics`
      command (assert on the deterministic transport double's command log).
- [x] Visual viewport on the response equals the pre-fetched rect.

## Implementation Order
1. Unit 1
2. Unit 2

## Testing
- Interface tests above on the deterministic doubles (layered-cdp-qualification
  base tier); no real-chrome tier addition needed — geometry decode is pure.
- Keep `geometry_over_cap_omits_layout_and_keeps_snapshot_available` as the
  document-anchor regression.

## Risks
- Timing: viewport rect is sampled just before DOMSnapshot capture; a scroll
  between the two samples skews selection. Mitigated by the existing document
  fingerprint staleness check and by margin-free intersection being best-effort
  presentation, not interaction authority (clicks re-resolve geometry).

## Implementation notes
- Execution capability: host implementation, because the existing decoder and scripted transport form one cohesive CDP boundary.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-cdp/src/control/snapshot.rs`.
- Tests added/removed: added over-cap viewport selection, semantic metadata, truncation accounting, and one-layout-metrics-fetch assertions; retained the document-anchor regression.
- Simplification: consolidated viewport metrics acquisition so the same rect feeds decode and response attachment; no second fetch or compatibility path remains.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `CARGO_TARGET_DIR=/tmp/krometrail-target cargo test -p krometrail-cdp control::snapshot::tests:: --locked` passed (27 tests).

## Review findings (cross-model, Fable reviewing Luna)

No blocking findings; implementation matches the design, acceptance criteria
verified against the committed tests, full workspace gate re-run independently.
