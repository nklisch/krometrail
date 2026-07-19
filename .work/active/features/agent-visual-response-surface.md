---
id: agent-visual-response-surface
kind: feature
stage: implementing
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Make the default agent surface visual, discoverable, and bounded

Correct the MCP presentation defects reproduced during comparative manual testing: chronological browser-event detail is projected away, all resolved-range follow-up schemas appear as opaque object unions, concise ranges repeat every frame UUID, and sanitized URL digests dominate compact inventories. Make visual operations include one useful image by default while retaining an explicit `inline_images: false` text-only override, and remove non-actionable root-document entries from the concise action target index.

## Source findings

- `idea-expose-browser-event-detail`
- `idea-compact-temporal-frame-ids`
- `idea-compact-sanitized-url-digests`
- Direct manual-test finding: retained image resources were produced but not visually inspected because omitted `inline_images` suppressed every pixel.
- Direct manual-test finding: `RootWebArea` can occupy prime concise target space despite not being a meaningful interaction target.

## Simplification opportunity

Keep one canonical result and one response projector. Treat `inline_images` as an optional override whose omitted value materializes from the operation kind, compact resolved-range and URL identity only in concise presentation, and generate concrete range-or-handle schema branches rather than maintaining opaque constraint-only unions.

## Design decisions

- **Image defaults**: omission follows operation purpose. Explicit visual tools (`take_screenshot`, `observe_live`, `temporal_debug_bundle`, `fetch_source_frames`, `generate_artifacts`, and `generate_region_filmstrip`) inline one bounded requested/primary image by default. Routine lifecycle, query, control, event, pin, and video tools remain text/resource-first. Explicit `inline_images: false` suppresses pixels and `true` requests them wherever supported.
- **Range schemas**: preserve exact either-range-or-handle validation while making both branches concrete. Each root union branch contains its complete properties and required fields; constraint-only opaque branches are removed.
- **Chronological events**: concise event detail keeps bounded event rows, cursor, warnings, and availability facts. Only duplicate canonical range/frame arrays are compacted.
- **URL identity**: use the existing validated lowercase-hex `Sha256Digest` as the one current sanitized path representation and bump the current store schema; do not add an array compatibility decoder.
- **Post-action semantics**: keep standalone concise snapshots interaction-only, while automatic live observations add a tiny bounded semantic-outcome list for alerts, dialogs, status roles, and named text. It describes current post-action state, not a pre/post diff.

## Architectural choice

Resolve operation defaults once at the registry/response boundary, then project canonical results. Reuse current artifact/source retrieval to populate one bounded inline image; use one compact-range projection everywhere; and make schema branches self-describing rather than weakening runtime validation. This keeps pixels, structured detail, resources, and canonical provenance independent without allowing a visual tool to hide its defining output by default.

## Implementation units

### Unit 1: Discoverable follow-up schemas and truthful event detail

**Story**: `agent-visual-response-surface-followup-contracts`

**Files**: `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/server.rs`

```rust
fn range_handle_input_schema(base: Arc<JsonObject>) -> Result<Arc<JsonObject>>;

fn compact_browser_event_detail(value: &Value) -> Result<Value, ResponseInvariantError>;
```

Build two complete object branches: the canonical-range branch and the range-handle branch. Add `response` to both through the shared schema projector. The chronological response keeps its already bounded `events` array and pagination authority.

**Acceptance criteria**:

- [ ] Tool schemas expose concrete `range`/`range_handle`, filters, selection, clip, focus times, and response preferences rather than `{unknown}` branches.
- [ ] Exactly one range authority remains schema- and runtime-enforced.
- [ ] Default `query_browser_events` contains event rows and next cursor while retaining bounded capture/gap warnings.

### Unit 2: Compact repeated temporal and URL identity

**Story**: `agent-visual-response-surface-compact-identities`

**Files**: `crates/krometrail-core/src/browser/privacy.rs`, `crates/krometrail-store/src/index/schema.rs`, `crates/krometrail-store/src/index/browser_events.rs`, `crates/krometrail-mcp/src/response.rs`

```rust
#[derive(Serialize)]
struct CompactResolvedRange {
    session_id: SessionId,
    target_id: TargetId,
    anchor_kind: TemporalRangeAnchorKind,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    frame_count: u32,
    interaction_count: u32,
    navigation_count: u32,
    marker_count: u32,
    gap_count: u32,
    retention_warning_count: u32,
    options: RangeResolutionOptions,
}
```

`SanitizedUrl` stores `Option<Sha256Digest>`. Concise bundle, event, artifact, frame-list, and frame-fetch projections use the compact range; expanded/full retain the canonical ordered IDs. Source-frame rows retain their own requested identifiers without duplicating them in the range.

**Acceptance criteria**:

- [ ] Sanitized URL path identity serializes as exactly 64 lowercase hexadecimal characters.
- [ ] Older incompatible stores reject under the bumped current schema with existing recovery guidance.
- [ ] Concise 29+ frame responses remain bounded and contain counts plus range-handle drill-down, while expanded/full preserve ordered IDs.

### Unit 3: Purpose-sensitive image defaults and bounded outcome context

**Story**: `agent-visual-response-surface-visual-defaults`

**Files**: `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/registry.rs`, `docs/VISION.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`

```rust
pub struct ResponseRequest {
    pub detail: ResponseDetail,
    pub inline_images: Option<bool>,
}

impl ResponseRequest {
    fn inline_images_for(self, operation: AgentOperationKind) -> bool;
}
```

Artifact and filmstrip mapping becomes asynchronous so it can reuse the retained artifact read path with existing byte/hash/dimension limits. Structural web-area/document roles cannot become actionable from generic focusable/clickable signals alone. Automatic live observations publish a small bounded `semantic_outcomes` projection from the already acquired snapshot.

**Acceptance criteria**:

- [ ] Omitted/false/true matrices match every operation's advertised image behavior; explicit visual tools return actual image content by default.
- [ ] Artifact and filmstrip `inline_images` is no longer ignored and validates retained identity before publishing bytes.
- [ ] Structural RootWebArea/document nodes do not displace real controls in concise targets.
- [ ] Live observations expose bounded current alerts/status/dialog/text context without claiming it changed.
- [ ] Skill instructions teach defaults, suppression, direct image inspection, and chronological event/range-handle drill-down.

## Implementation order

1. Repair range-handle schemas and chronological event projection.
2. Compact repeated range/URL identities and bump the current store schema.
3. Materialize purpose-sensitive image defaults, artifact reads, structural actionability, semantic outcomes, and documentation.

## Simplification

- One operation-purpose resolver replaces scattered boolean defaults.
- One compact-range helper replaces repeated full `ResolvedRange` serialization in concise paths.
- The existing digest type replaces the parallel raw-byte digest representation.
- Remove the duplicate `project_page_state_part` call in live-observation projection while touching that boundary.

## Testing

- Schema tests assert complete concrete branches and exact-one validation.
- MCP response/server tests protect event rows, compact size, image content/default overrides, artifact identity, and semantic outcomes.
- Core/store tests protect current digest serialization and incompatible-store rejection.
- CDP decoder tests use a real RootWebArea-shaped node plus real controls.
- Regenerate checked-in public MCP schemas and `docs/public/llms-full.txt` from their authorities.

## Risks

Default image reads add bounded I/O to explicit visual tools and may degrade when retained bytes become unavailable; the structured result and canonical resource must remain successful with a warning. Concrete schema unions must remain acceptable to MCP clients while no longer collapsing to opaque branches. A current-store bump intentionally rejects existing local evidence, so the recovery message and installer/plugin docs must remain clear.
