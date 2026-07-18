---
id: epic-agent-browser-ergonomics-browser-contexts
kind: feature
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics
depends_on: [epic-agent-browser-ergonomics-semantic-targeting]
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Browser contexts and assets

## Brief

Expose the browser contexts agents need to navigate deliberately: list reusable named managed profiles without paths, preserve popup opener relationships and wait for newly created pages, inventory frames, scope semantic inspection and interaction to qualified same-origin frames, and list privacy-bounded current-page asset metadata. Update the agent skill to explain reusable named profiles and when to choose them instead of temporary profiles or attachment.

The work excludes raw resource bodies, unrestricted DevTools commands, cross-origin frame DOM access, and unqualified OOPIF interaction. Unsupported browser/context variants fail explicitly rather than falling back to main-document coordinates.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: consumer of main-document semantic targeting and producer of explicit browser-context scope

## Simplification opportunity

Reuse target supervision for popup/frame identity, sanitized network/resource metadata for assets, and the existing managed-profile launcher. Do not create a second browser automation layer.

## Foundation references

- `docs/SPEC.md` — Browser-Control Surface and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Target Lifecycle, MCP Boundary, and Observability

## Design decisions

- **Public grouping**: keep `list_pages` unchanged and add `list_page_contexts` plus `wait_for_page`; this preserves the stable array result while giving new callers a monotonic page cursor and opener metadata.
- **Profile authority**: profile discovery is a browser-connector lifecycle query available without an active session. It returns validated identities and an `in_use` flag only—never profile paths, sizes, visited origins, cookies, or timestamps.
- **Frame authority**: expose opaque frame references tied to target attachment and frame-tree generation. Only same-origin, same-process descendants are inspectable/actionable; cross-origin, OOPIF, detached, and indeterminate frames remain inventory entries with explicit unsupported reasons.
- **Frame actions**: semantic queries still resolve to exact `NodeReference` values. The snapshot registry binds those nodes to a qualified frame internally and translates same-process frame geometry to root viewport coordinates; actions do not gain persistent locator identities.
- **Asset source**: `list_page_assets` reads the current document's bounded Resource Timing projection on demand. It exposes sanitized URL metadata, initiator kind, duration, and browser-disclosed sizes, never headers, bodies, query strings, response contents, or local paths.
- **Mockups**: no UI surface; MCP contracts and skill guidance only.
- **Dispatch**: direct-read design over the existing target reducer, snapshot registry, profile lease, operation registry, MCP lifecycle/resource projection, and browser fixtures.

## Architectural choice

Three approaches were considered. A Playwright-like parallel context layer would provide familiar locators but duplicate target and snapshot authority and weaken explicit stale-reference behavior. Exposing raw CDP target/frame/resource commands would be small internally but would leak unstable identifiers and cross-origin authority into the public contract. The chosen approach extends the existing typed target supervisor and snapshot registry with privacy-bounded projections: connector-owned profile inventory, supervisor-owned page relationships/cursors, registry-owned qualified frames, and a bounded current-page asset operation. It adds convenience without creating a second automation system.

The trickiest unit is frame qualification and geometry. A frame may be same-origin but out of process, may navigate between inventory and action, or may disappear while a semantic query resolves. The adapter therefore issues opaque generation-scoped references, revalidates origin/process/attachment at every use, and fails before dispatch when it cannot prove the supported boundary.

## Implementation Units

### Unit 1: Privacy-bounded managed profile inventory

**Files**: `crates/krometrail-core/src/browser/target.rs`, `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-cdp/src/launcher/profile.rs`, `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-mcp/src/registry.rs`
**Story**: `epic-agent-browser-ergonomics-browser-contexts-profile-inventory`

```rust
// krometrail-core/src/browser/target.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagedProfileSummary {
    pub identity: ProfileIdentity,
    pub in_use: bool,
}

// krometrail-core/src/ports/browser.rs
pub trait BrowserConnector: Send + Sync {
    fn installations(&self) -> PortFuture<'_, Result<Vec<BrowserInstallation>>>;
    fn managed_profiles(&self) -> PortFuture<'_, Result<Vec<ManagedProfileSummary>>>;
    fn connect(&self, request: BrowserConnectRequest)
        -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>>;
}

// krometrail-cdp/src/launcher/profile.rs
pub fn list_reusable_profiles(root: &Path) -> Result<Vec<ManagedProfileSummary>, ProfileError>;
```

**Implementation notes**:
- Add the lifecycle tool `list_managed_profiles` through the existing MCP lifecycle registry. It is valid before, during, or after a session and delegates once to `BrowserConnector::managed_profiles`.
- Enumerate only direct, non-symlink directories under `<profile-root>/profiles`; accept names through the existing `ProfileIdentity`/profile-name validation and sort by identity. Presence of `.krometrail.lock` reports `in_use`; it does not reveal a PID or path.
- A missing profile directory is an empty inventory. Root access failure is a stable `page_observation_failed`-class connector error with correlation-only diagnostics.

**Acceptance criteria**:
- [x] The default and user-named reusable profiles are discoverable by identity after creation, while temporary profiles and invalid/symlink entries are absent.
- [x] The result contains no filesystem path, origin, cookie, timestamp, byte count, or browser content.
- [x] The tool remains callable with no active browser and canonical generated schemas include it once.

### Unit 2: Popup relationships and race-safe page waits

**Files**: `crates/krometrail-core/src/browser/contexts.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-cdp/src/targets/model.rs`, `crates/krometrail-cdp/src/targets/reducer.rs`, `crates/krometrail-cdp/src/session/operations.rs`, `crates/krometrail-mcp/src/response.rs`
**Story**: `epic-agent-browser-ergonomics-browser-contexts-page-relationships`

```rust
pub const MAX_KNOWN_PAGE_TARGETS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct PageSequence(NonZeroU64);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageContextStatus {
    pub page: PageStatus,
    pub sequence: PageSequence,
    pub opener_target_id: Option<TargetId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageContextInventory {
    pub cursor: PageSequence,
    pub pages: Vec<PageContextStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListPageContextsRequest {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaitForPageRequest {
    pub after: PageSequence,
    pub opener_target_id: Option<TargetId>,
    pub timeout: DurationMillis,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WaitForPageResult {
    pub matched: PageContextStatus,
    pub cursor: PageSequence,
}
```

**Implementation notes**:
- Decode CDP `TargetInfo.openerId` into transport state, then resolve it to a Krometrail target only when both targets belong to the current supervised browser generation. Never expose the raw opener key.
- Assign a monotonically increasing non-zero page sequence when a logical target is first discovered; preserve it through info changes and attachment. `list_page_contexts` is the race-free cursor source.
- `wait_for_page` first checks retained supervisor state for the lowest-sequence page newer than `after`, then subscribes to revisioned target events. Optional opener matching never falls back to an unrelated page. Subscriber lag returns `browser_disconnected` with refresh-and-retry guidance.
- Both operations are browser-scoped, read-only, non-batchable registry entries. `list_pages` retains its exact 1.x response.

**Acceptance criteria**:
- [x] A popup created between inventory and wait is returned immediately and carries its opener's Krometrail target ID.
- [x] Multiple popups are returned deterministically by sequence; unrelated or pre-cursor pages do not satisfy the wait.
- [x] Reconnect does not invent a new logical popup relationship, and stale/unresolvable raw opener IDs project as `None`.
- [x] Timeout, subscriber lag, target closure, and session shutdown return stable errors without activating or focusing a page.

### Unit 3: Qualified frame inventory and frame-scoped semantic references

**Files**: `crates/krometrail-core/src/browser/contexts.rs`, `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-cdp/src/control/frames.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-cdp/src/control/pointer.rs`, `crates/krometrail-cdp/src/control/mod.rs`
**Story**: `epic-agent-browser-ergonomics-browser-contexts-frame-scope`

```rust
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageFrameReference {
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub frame_generation: NonZeroU64,
    pub frame_key: NonEmptyText, // adapter-issued opaque token, not a CDP frame id
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameAccess {
    MainDocument,
    SameOriginSameProcess,
    CrossOrigin,
    OutOfProcess,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageFrameStatus {
    pub reference: PageFrameReference,
    pub parent: Option<PageFrameReference>,
    pub depth: u16,
    pub access: FrameAccess,
    pub url: SanitizedUrl,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListFramesRequest { #[serde(default)] pub target: PageSelection }

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageFrameInventory {
    pub target_id: TargetId,
    pub frames: Vec<PageFrameStatus>,
    pub omitted_frame_count: u32,
}

// additive extension to the semantic-targeting request designed by the dependency
pub enum SemanticDocumentScope {
    MainDocument,
    Frame(PageFrameReference),
}
```

**Implementation notes**:
- Add `list_frames` as a page-scoped read-only operation. Read `Page.getFrameTree`, derive origin from CDP security-origin data (including inherited `about:blank`/`srcdoc` ancestry), and keep raw CDP frame IDs only in the adapter registry.
- The registry maps an opaque random/hash token to raw frame identity, loader/document fingerprint, owner backend node, parent, target attachment, origin qualification, and process qualification. Navigation, detach, OOPIF migration, or frame removal invalidates the generation.
- Only `MainDocument` and `SameOriginSameProcess` pass semantic snapshot/query. Cross-origin and OOPIF inventory remains visible but returns `unsupported` before DOM/AX access or input dispatch; no coordinate fallback is attempted.
- Frame snapshots use `Accessibility.getFullAXTree(frameId)` and merge bindings into the target's active snapshot generation without changing the public `NodeReference`. Each binding retains its frame registry key. Pointer geometry walks owner-frame quads to the root viewport; form/keyboard resolution revalidates the same frame chain immediately before dispatch.
- Bound the inventory to 256 preorder entries and report omissions. URLs use `SanitizedUrl`; names, DOM, accessible text, and origins beyond that sanitizer are not included in inventory.

**Acceptance criteria**:
- [x] Main-document and nested same-origin/same-process queries return exact actionable references whose actions land on the intended element.
- [x] A frame navigation, removal, target reattach, or OOPIF migration makes old frame/reference input fail before dispatch with `stale_reference` or `unsupported` and concrete recovery.
- [x] Cross-origin and OOPIF frames can be inventoried but neither inspected nor interacted with, and no main-document coordinate fallback occurs.
- [x] Nested-frame pointer coordinates are translated once and remain correct after root-page scrolling.

### Unit 4: Bounded page asset metadata and agent guidance

**Files**: `crates/krometrail-core/src/browser/assets.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-cdp/src/control/assets.rs`, `crates/krometrail-mcp/src/response.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/setup.md`
**Story**: `epic-agent-browser-ergonomics-browser-contexts-assets-guidance`

```rust
pub const MAX_PAGE_ASSETS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageAssetKind { Script, Stylesheet, Image, Font, Media, Fetch, XmlHttpRequest, Other }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageAssetMetadata {
    pub url: SanitizedUrl,
    pub kind: PageAssetKind,
    pub duration_ms: f64,
    pub transfer_bytes: Option<u64>,
    pub encoded_body_bytes: Option<u64>,
    pub decoded_body_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListPageAssetsRequest { #[serde(default)] pub target: PageSelection }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageAssetInventory {
    pub target_id: TargetId,
    pub assets: Vec<PageAssetMetadata>,
    pub omitted_asset_count: u32,
}
```

**Implementation notes**:
- Evaluate a fixed, side-effect-free adapter-owned expression over `performance.getEntriesByType('resource')`; callers cannot supply script. Validate every number as finite/non-negative and sanitize each URL before constructing domain values.
- Sort deterministically by start time then sanitized URL digest, take 256, and report omissions. Zero browser-disclosed sizes remain `Some(0)`; unavailable values are `None`.
- Do not expose query/fragment, headers, cookies, initiator stacks, response bodies, DOM text, filenames, local paths, or raw URLs. Do not add asset records to browser-event evidence or logs.
- Update the installed skill to teach `list_managed_profiles`, cursor-based popup waits, frame support/failures, and asset metadata limits.

**Acceptance criteria**:
- [x] A fixture with script, stylesheet, image, font/fetch, and cross-origin assets returns bounded sanitized metadata with no resource bytes.
- [x] Oversized inventories truncate deterministically and malformed entries are omitted with a count rather than leaking raw data.
- [x] Generated schema/registry tests cover every new operation and the skill accurately describes the shipped surface.

## Implementation order

1. Profile inventory and page relationship/cursor contracts.
2. Frame registry qualification and frame-scoped semantic integration after the semantic-targeting dependency lands.
3. Asset metadata and skill guidance.
4. Canonical schema regeneration and real-browser qualification across popups, frames, reconnect, scrolling, and privacy failures.

## Simplification

- Retain `list_pages` and the current `NodeReference`; do not create a second locator or page identity system.
- Reuse profile validation/locking, target reduction, snapshot bindings, `SanitizedUrl`, operation registry, and MCP projector.
- Do not persist raw opener IDs, frame IDs, asset URLs, or a second network catalog.
- No existing tests are removed; new interface fixtures should extend the browser lifecycle/observation fixtures instead of creating another browser harness.

## Testing

- Core contract tests protect validation, serde, page sequence ordering, opaque frame scoping, asset bounds, and canonical registry completeness.
- Reducer/session tests protect popup races, opener resolution, reconnect identity, subscriber lag, and no-focus behavior.
- Scripted CDP tests protect frame-tree qualification, same-origin nested geometry, cross-origin/OOPIF rejection, stale generation fences, and sanitized Resource Timing decoding.
- One real-Chrome fixture test covers popup creation, nested same-origin interaction, cross-origin explicit failure, asset listing, and no raw IDs/paths in responses or correlated logs.
- Generated MCP schema fixtures and plugin static checks protect the public tool/skill contract.

## Risks

- **Riskiest assumption**: CDP exposes enough stable owner-frame geometry for nested same-process frames across supported Chrome versions. If qualification disproves this, keep inventory and semantic inspection but return `unsupported` for pointer actions in child frames; never guess coordinates.
- Same-origin can change during a race. Every query and action revalidates the frame generation and security origin immediately before accessing DOM or dispatching input.
- OOPIF representation differs by Chrome version. Indeterminate qualification fails closed and remains visible in inventory.
- Resource Timing is page-controlled and may omit sizes due to browser privacy rules. Results describe browser-disclosed metadata only and never claim a complete network log.

## Implementation notes

- Execution capability: one direct feature owner with serialized coordination around the shared core/CDP/MCP registries and local-I/O feature work.
- Delivered all four designed units through the existing connector, target reducer, page-control, operation registry, MCP projector, and installed skill; no parallel browser automation or locator authority was introduced.
- Compact ergonomic defaults remain unchanged: callers opt into context/profile/frame/asset detail only when needed, page selection remains implicit where supported, and response expansion stays explicit.
- Generation safety was tightened during implementation: popup relationships retain resolved target identities instead of dynamically rebinding raw keys, and frame tokens hash adapter-private frame plus loader identity so navigation invalidates old scope.
- Frame process qualification cross-checks browser `iframe` targets. An unavailable process inventory marks descendants indeterminate and fails closed; cross-origin and OOPIF scope never falls back to main-document inspection or coordinates.
- `wait_for_page` uses bounded browser inventory polling because the serialized operation owner cannot consume supervisor events while awaiting its own completion. Reconciliation still flows through the single-writer reducer and applies attach effects without activation/focus.
- The installed skill defaults to reusable `default`, cursor-first popup waits, main-document semantic scope, and small bounded asset reads, with explicit expansion to named profiles, frames, and metadata.

## Verification

- `cargo check --workspace --all-targets --locked`
- `cargo test -p krometrail-core browser::contexts::tests`
- `cargo test -p krometrail-core browser::operation::tests::declaration_is_the_complete_operation_registry`
- `cargo test -p krometrail-cdp control::contexts::tests`
- `cargo test -p krometrail-cdp --test target_reducer`
- `cargo test -p krometrail-mcp schema`
- `cargo test -p krometrail-mcp route_registry`

## Review handoff

- Review weight: standard feature review.
- Pay particular attention to frame process/generation qualification, popup-key reuse, wait polling lifecycle behavior, profile filesystem privacy, and Resource Timing omission accounting.

## Standard review — 2026-07-18

- Outcome: approved after requested corrections.
- Page cursors now use a non-rewinding high-water mark with a reserved empty-inventory cursor, and page waits stop promptly when the session or transport shuts down.
- Popup opener identity is immutable after first resolution, including reconnect and raw target-key reuse.
- Frame-scoped semantic snapshots retain the qualified frame authority, select the exact child document, inherit `about:blank`/`about:srcdoc` origins, and revalidate frame, loader, document, attachment, and process qualification before resolving a node for interaction or waits.
- Resource Timing is sorted and capped in the browser-owned expression before crossing the transport boundary, with explicit omission accounting.
- Registry declarations were deduplicated and default enum behavior derives from the canonical variant.
- Review evidence: focused child-document/frame-origin tests, the 10-case target reducer suite, `cargo check -p krometrail-cdp --all-targets --locked`, and `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` all pass.
