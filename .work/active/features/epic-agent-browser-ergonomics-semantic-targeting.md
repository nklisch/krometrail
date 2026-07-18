---
id: epic-agent-browser-ergonomics-semantic-targeting
kind: feature
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Semantic query-to-reference targeting

## Brief

Let agents find exact actionable references through bounded semantic queries over the current main-document page snapshot: accessible role/name, label text, visible text, and test identifier, with descendant scope. The query returns zero, one, or bounded-many explicit matches and never silently chooses among ambiguous nodes. Existing mutation tools continue accepting exact references, preserving stale-reference and pre-dispatch safety semantics.

This feature does not add persistent locator identities or automatic action-time reevaluation. Frame identity and scope belong to the dependent browser-context feature; DOM enrichment required for label and test-id matching belongs in the existing snapshot registry.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent targeting foundation consumed by browser-context discovery

## Simplification opportunity

Use one registry-derived read-only browser operation and the existing generation-scoped node reference authority instead of adding Playwright-like locator objects to every action schema.

## Foundation references

- `docs/SPEC.md` — Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Domain Model and Target Lifecycle

## Design decisions

- **Resolution model**: add one read-only `query_page` registry operation that captures/refreshes the current snapshot and returns exact `NodeReference` values; mutation schemas remain unchanged — this removes the routine full-snapshot round trip without creating persistent or action-time-re-evaluated locators.
- **Ambiguity**: zero, one, many, and bounded-overflow are successful explicit outcomes (`no_match`, `unique`, `ambiguous`, `truncated`); only a caller holding exactly one returned reference may proceed — this exposes all safe choices and never silently chooses or turns normal ambiguity into a transport failure.
- **Matching defaults**: accessible role and test identifier are exact; accessible-name, label, and visible-text matches normalize Unicode whitespace, default to case-insensitive exact comparison, and opt into `contains` explicitly — deterministic defaults are more useful for actions than fuzzy matching.
- **Scope and freshness**: optional descendant scope is an existing `NodeReference`; refreshing the same attached document preserves its generation/node identity, while navigation, reconnect, backing-node replacement, or an absent scope fails `stale_reference` before query results are returned.
- **DOM enrichment**: keep label, rendered text, and test-id metadata private to the active snapshot registry and derive it from one bounded `DOMSnapshot.captureSnapshot` response joined by backend node id — public `PageSnapshot` stays compact and there is no second reference authority.
- **Actionability**: only snapshot nodes that already carry an actionable reference are candidates; text on descendants is aggregated onto their actionable ancestor, while generic/non-actionable containers are not promoted into interaction targets.

## Architectural choice

Three approaches were considered. Extending every mutation with a Playwright-like locator would reduce calls most aggressively, but would duplicate schemas and make ambiguity/staleness depend on action-time page state. Returning persistent locator objects would be composable, but creates a second identity and lifecycle system alongside generation-scoped references. The selected approach is one registry-derived read-only query operation over the existing snapshot/reference registry. It adds a small public contract, makes query freshness and ambiguity explicit, and leaves every mutation's pre-dispatch safety boundary unchanged.

The trickiest unit is the AX/DOM join: accessible role/name comes from `Accessibility.getFullAXTree`, while label associations, test identifiers, and rendered descendant text come from DOM data. The adapter will decode one bounded `DOMSnapshot.captureSnapshot` payload into private metadata keyed by `backendNodeId`, then install that metadata atomically with the AX snapshot. If Chrome cannot provide a structurally valid join, the query fails `page_observation_failed`; it never falls back to partial matching that could select a different element.

## Implementation Units

### Unit 1: Validated semantic query contract and registry operation

**Files**: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/browser/batch.rs`

**Story**: `epic-agent-browser-ergonomics-semantic-targeting-query-contract`

```rust
pub const DEFAULT_SEMANTIC_MATCH_LIMIT: u16 = 20;
pub const MAX_SEMANTIC_MATCH_LIMIT: u16 = 100;
pub const MAX_SEMANTIC_QUERY_TEXT_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTextMatchMode { #[default] Exact, Contains }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct SemanticTextMatch {
    pub value: NonEmptyText,
    pub mode: SemanticTextMatchMode,
    pub case_sensitive: bool,
}

impl SemanticTextMatch {
    pub fn new(
        value: impl Into<String>,
        mode: SemanticTextMatchMode,
        case_sensitive: bool,
    ) -> Result<Self>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticQuery {
    Role { role: NonEmptyText, name: Option<SemanticTextMatch> },
    Label { text: SemanticTextMatch },
    Text { text: SemanticTextMatch },
    TestId { value: NonEmptyText },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct QueryPageRequest {
    pub target: PageSelection,
    pub query: SemanticQuery,
    pub scope: Option<NodeReference>,
    pub max_matches: u16,
}

impl QueryPageRequest {
    pub fn new(
        target: PageSelection,
        query: SemanticQuery,
        scope: Option<NodeReference>,
        max_matches: u16,
    ) -> Result<Self>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticQueryOutcome { NoMatch, Unique, Ambiguous, Truncated }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SemanticMatch {
    pub reference: NodeReference,
    pub role: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct QueryPageResult {
    pub context: ObservationContext,
    pub generation: SnapshotGeneration,
    pub outcome: SemanticQueryOutcome,
    pub matches: Vec<SemanticMatch>,
    pub omitted_match_count: u32,
}

// Registry addition:
BrowserOperationKind::QueryPage(QueryPageRequest) -> QueryPageResult
// stable_name: "query_page", read-only, page-scoped, requested-only, batchable
```

**Implementation notes**:

- Implement validated custom deserialization for `SemanticTextMatch`, `SemanticQuery`, and `QueryPageRequest`: reject text over 1,024 UTF-8 bytes, roles containing whitespace/control characters, role values that are not lowercase ASCII, and match limits outside `1..=100`. Omitted `target`, `mode`, `case_sensitive`, and `max_matches` default to selected, exact, false, and 20 respectively.
- `TestId` compares the raw `data-testid` value byte-for-byte and is always case-sensitive. Text comparison trims and collapses each Unicode whitespace run to one ASCII space; case-insensitive matching compares Rust Unicode lowercase expansions rather than locale-dependent browser casing.
- Construct `outcome` from the complete candidate count: zero=`no_match`, one=`unique`, `2..=max_matches`=`ambiguous`, over limit=`truncated`; retain the first `max_matches` matches in document preorder and report the exact saturated `omitted_match_count`.
- Add `QueryPage` once to `define_browser_operations!`, `BrowserOperationRequest`/`Result`, page-selection inheritance, and batch validation. Do not hand-enumerate a second MCP route.

**Acceptance criteria**:

- [ ] Generated JSON schema publishes all four query variants, defaults, text/match bounds, optional descendant scope, and `1..=100` match limit.
- [ ] Existing operation names, request shapes, snapshot serialization, and mutation locator schemas remain byte-compatible when the new operation is unused.
- [ ] Domain tests prove whitespace normalization, Unicode case handling, invalid role/text/limit rejection, and all four outcome invariants.

### Unit 2: Atomic snapshot enrichment and bounded matching

**Files**: `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-cdp/src/control/mod.rs`, `crates/krometrail-cdp/src/session/operations.rs`, `crates/krometrail-cdp/src/control/tests.rs`

**Story**: `epic-agent-browser-ergonomics-semantic-targeting-query-resolution`

```rust
#[derive(Clone, Debug, Default)]
struct SemanticNodeMetadata {
    labels: Vec<String>,
    rendered_text: String,
    test_id: Option<String>,
}

struct ActiveSnapshot {
    // existing generation/document/bindings fields
    semantic: HashMap<SnapshotNodeId, SemanticNodeMetadata>,
    parent_by_node: HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
}

impl PageControl {
    async fn query_page(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: QueryPageRequest,
        started_at: SessionTime,
    ) -> Result<BrowserOperationResult>;
}

impl SnapshotRegistry {
    fn query(
        &self,
        bound: &BoundTarget,
        request: &QueryPageRequest,
        snapshot: &PageSnapshot,
    ) -> Result<QueryPageResult>;
}
```

**Implementation notes**:

- Factor the existing snapshot capture into one private routine used by `snapshot_page`, live observation, and `query_page`. For semantic capture, issue `DOMSnapshot.captureSnapshot` with no computed styles and bounded optional data disabled; reject more than 5,000 DOM nodes or 1 MiB of decoded semantic text using the existing snapshot limits.
- Decode the DOMSnapshot string table and parallel node/layout arrays with strict index validation. Build backend-id metadata for raw `data-testid`, explicit `<label for=id>`, wrapping `<label>`, `aria-labelledby`, and rendered layout text. Do not persist arbitrary attributes or page HTML.
- Aggregate normalized rendered descendant text only onto actionable AX nodes, cap each value at the query text limit, and preserve AX preorder. Duplicate label strings are deduplicated without changing first-seen order.
- Install the AX nodes, actionable bindings, parent map, and semantic metadata together only after both payloads validate. A failed DOM capture leaves the prior active snapshot unchanged and returns `page_observation_failed`.
- Validate `scope` through the existing document, attachment-generation, active-generation, and binding authority before matching. A node matches only when it is a strict descendant of the scoped node; the scope itself is excluded.
- Role matching uses the AX role/name. Label, text, and test-id matching use private semantic metadata. Every returned match copies the exact current-generation `NodeReference`; query resolution performs no interaction actionability check beyond the snapshot's existing actionable predicate.

**Acceptance criteria**:

- [ ] Scripted CDP tests cover role/name, explicit and wrapping labels, visible descendant text, test ID, strict descendant scope, preorder, duplicate strings, and bounded overflow.
- [ ] A scope from another target/document/generation fails `stale_reference`; no partial or unrelated results are returned.
- [ ] Malformed/oversized DOMSnapshot input fails closed without replacing the previously active registry generation.
- [ ] A repeated query on the unchanged document preserves reference identity; navigation, reconnect, or backing-node replacement invalidates old scope/references before action dispatch.

### Unit 3: MCP schema/projection and real-browser qualification

**Files**: `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-cdp/tests/verified_interactions.rs`, `tests/fixtures/browser/verified-interactions/index.html`, `plugin/skills/krometrail/SKILL.md`

**Story**: `epic-agent-browser-ergonomics-semantic-targeting-query-resolution`

```rust
// Registry-derived MCP tool:
// query_page(QueryPageRequest) -> ToolResponse.result = QueryPageResult
```

**Implementation notes**:

- Let the existing operation registry generate and route `query_page`; the response projector serializes the bounded result with no screenshot or full `PageSnapshot` image/content block.
- Extend the verified-interactions fixture with repeated accessible names, nested scopes, explicit/wrapping labels, rendered descendant text, and `data-testid` values. Keep fixture additions deterministic and framework-free.
- Update the plugin skill to prefer `query_page` for routine role/name/label/text/test-id targeting, branch on `outcome`, use only a unique exact reference, and refresh after `stale_reference`.

**Acceptance criteria**:

- [ ] MCP registry/schema tests prove exactly one new `query_page` route, batch request support, and a bounded image-free response.
- [ ] Real Chrome tests resolve each query kind, expose ambiguity without choosing, use a unique result in an existing mutation, and reject the old reference after navigation.
- [ ] Plugin guidance does not imply automatic locator reevaluation or cross-origin/frame support from this feature.

## Implementation order

1. `epic-agent-browser-ergonomics-semantic-targeting-query-contract` establishes validated public shapes and the registry declaration.
2. `epic-agent-browser-ergonomics-semantic-targeting-query-resolution` implements atomic AX/DOM capture, matching, MCP projection, real-browser tests, and agent guidance.

## Simplification

- Reuse and factor the current snapshot capture/install path instead of creating a locator cache, query service, or action-time locator resolver.
- Keep semantic DOM data private and bounded rather than expanding every serialized `SnapshotNode` with label/test-id/text fields.
- Derive the MCP tool and batch shape from `BROWSER_OPERATION_REGISTRY`; remove any query-specific routing/schema enumeration encountered during implementation.
- Retain CSS selectors as the debugging escape hatch and exact references as the sole mutation authority.

## Testing

- Interface tests protect the additive registry/schema and stable existing request shapes.
- Scripted CDP decoder tests protect malformed parallel-array handling, DOM/AX joins, scope fencing, ordering, and bounds without requiring Chrome.
- One real-browser scenario protects browser-protocol assumptions and the query-to-reference-to-action workflow.
- Do not duplicate all query cases at every layer: normalization belongs in core tests, join/matching cases in scripted adapter tests, and only representative end-to-end cases in real Chrome.

## Risks

- `DOMSnapshot.captureSnapshot` array/string-table shapes and rendered-text coverage vary across Chrome revisions. The strict decoder plus one real-browser qualification is the gate; if layout text proves incomplete, fall back to an equally bounded read-only DOM runtime projection while retaining the same private metadata and public query contract.
- Large documents could make semantic capture expensive. Existing 5,000-node/1 MiB limits remain the first bound; operation timing should be measured in the real-browser test before considering a separate performance item.
- Label/text aggregation can overmatch nested controls. Matching remains restricted to actionable AX nodes, returns ambiguity explicitly, and preserves exact references so the caller must narrow rather than Krometrail guessing.
