---
id: browser-runtime-manual-test-hardening
kind: feature
stage: done
tags: [browser, agent-ux, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Harden browser runtime behavior found through comparative manual testing

Fix the remaining runtime defects reproduced while comparing Krometrail with the in-app Browser and Chrome: frame-heavy capture acknowledgement failure, frame-scoped queries being rejected by unrelated main-document size, interaction-relative temporal ranges extending beyond the latest retained damage frame, and preserve-focus sessions requiring destructive restart when an agent deliberately needs to foreground one controlled page.

## Source findings

- `idea-frame-heavy-ack-regression`
- `idea-frame-query-global-cap`
- `idea-clamp-interaction-capture-tail`
- `idea-runtime-focus-escalation`

## Simplification opportunity

Keep capture acknowledgement, document-scoped snapshot authority, temporal range resolution, and target activation in their existing owners. Prefer one-shot explicit foreground activation over a second mutable session-policy system, and delete any recovery prose that still instructs agents to restart a healthy managed session.

## Design decisions

- **Acknowledgement timeout**: preserve the one-second, one-shot acknowledgement contract. A terminal acknowledgement failure enters the existing generation-fenced reconnect path instead of leaving capture permanently failed; raising the deadline only moves the cliff.
- **Frame query acquisition**: acquire DOM semantic metadata only for query variants that require label, rendered-text, test-id, or container-text data. Role/name queries use the already frame-scoped accessibility tree and retain completeness enforcement.
- **Natural interaction tail**: `AllowPartial` may clamp only naturally derived interaction/latest-interaction edges to retained capture bounds. Explicit ranges and `RequireComplete` remain exact failures; requested provenance remains unchanged.
- **Focus escalation**: add a deliberate one-shot `activate_page` operation. It foregrounds one selected target without mutating the session's `preserve` policy, so later hidden-page work remains protected.

## Architectural choice

Repair each behavior inside its existing authority: capture failures notify session supervision, semantic-query requirements choose the acquisition parts they actually consume, range resolution intersects only eligible natural anchors, and page control owns explicit activation. This is preferable to longer acknowledgement deadlines, larger global snapshot limits, silent temporal clamping, or mutable focus policy because those alternatives weaken failure truthfulness or create a second authority.

## Implementation units

### Unit 1: Recover capture after terminal acknowledgement failure

**Story**: `browser-runtime-manual-test-hardening-capture-reconnect`

**Files**: `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/capture/mod.rs`, `crates/krometrail-cdp/tests/session_supervision.rs`

```rust
async fn fail_acknowledgement(
    &mut self,
    error: KrometrailError,
    generation: u64,
) -> Result<()>;
```

The terminal path records exactly one acknowledgement gap/failure and signals the same generation-fenced connection-loss observer used when the frame event stream closes. Reconnect rebuilds the target and capture stream; it never retries an ambiguous acknowledgement token.

**Acceptance criteria**:

- [x] A deadline-exceeded acknowledgement sends exactly one acknowledgement command and records exactly one explicit gap.
- [x] Session supervision reconnects and a later attachment generation persists frames.
- [x] Queue depth and persistence remain outside acknowledgement timing, and the original failure remains visible in evidence quality.

### Unit 2: Acquire only semantic data required by the query

**Story**: `browser-runtime-manual-test-hardening-frame-query`

**Files**: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-cdp/tests/page_observation.rs`

```rust
impl SemanticQuery {
    pub const fn requires_dom_semantics(&self) -> bool;
}

async fn capture_snapshot_for_frame(
    &mut self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    started_at: SessionTime,
    include_dom_semantics: bool,
    frame: Option<&ResolvedFrameDocument>,
) -> Result<PageSnapshot>;
```

Role/name queries skip `DOMSnapshot.captureSnapshot`; label, rendered-text, test-id, and container-text queries retain it. Errors include bounded selected-document facts without raw frame IDs, page content, or URLs.

**Acceptance criteria**:

- [x] A tiny child-frame role/name query succeeds when the unrelated parent DOM exceeds 5,000 nodes.
- [x] Role/name queries do not issue DOMSnapshot capture, while semantic variants still do.
- [x] Any actual selected-frame or accessibility-tree limit failure identifies the failing acquisition and bounded node count with concrete narrowing guidance.

### Unit 3: Clamp eligible natural interaction tails

**Story**: `browser-runtime-manual-test-hardening-interaction-tail`

**Files**: `crates/krometrail-core/src/timeline/range.rs`, `crates/krometrail-store/tests/range_resolution.rs`

```rust
fn clamp_natural_interaction_range(
    seed: &RangeSeed,
    requested: SessionRange,
    retained: SessionRange,
    options: RangeResolutionOptions,
) -> Result<Option<SessionRange>>;
```

An eligible nonempty intersection becomes the resolved range, while the original requested range, interaction identity, effective anchor clamping, and a captured-bound warning remain explicit.

**Acceptance criteria**:

- [x] A latest static interaction whose completion is 26 ms after the last retained frame resolves under `AllowPartial` with `after_ms: 0`.
- [x] Explicit intervals, `RequireComplete`, and wholly disjoint natural ranges still fail.
- [x] No frame, stability, or continuity is invented beyond retained evidence.

### Unit 4: Explicitly foreground one page

**Story**: `browser-runtime-manual-test-hardening-activate-page`

**Files**: `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-core/src/browser/action.rs`, `crates/krometrail-cdp/src/session/operations.rs`, `crates/krometrail-cdp/src/control/interaction.rs`, `crates/krometrail-mcp/src/registry.rs`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `plugin/skills/krometrail/SKILL.md`

```rust
pub struct ActivatePageRequest {
    pub target: Option<TargetId>,
}

async fn activate_page(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    cancellation: &OperationCancellation,
) -> Result<PageOperationResult>;
```

The operation sends `Target.activateTarget` and `Page.bringToFront`, waits boundedly for visible document state, and returns the normal live observation. It is intentionally foregrounding even in `preserve` sessions but does not modify that immutable policy.

**Acceptance criteria**:

- [x] Explicit activation emits both foreground commands and returns a live observation after bounded visibility acknowledgement.
- [x] Failure remains `target_hidden` with no pointer event dispatched.
- [x] A later different hidden target is still protected by `preserve` and emits no activation/input until explicitly activated.

## Implementation order

1. Restore capture through supervised reconnect after acknowledgement failure.
2. Make semantic acquisition query-sensitive and qualify the frame regression.
3. Clamp only eligible natural interaction tails.
4. Add explicit page activation and roll focus documentation forward.

## Testing

- Capture pipeline and supervision tests protect one-shot acknowledgement plus recovery across attachment generations.
- CDP scripted tests protect exact query-dependent command sequences and frame selection.
- Core/store range tests protect requested-versus-resolved provenance and exact failure boundaries.
- Registry and page-control tests protect the new operation's schema, explicit focus effect, and immutable preserve policy.

## Risks

The acknowledgement failure may reflect a congested shared transport; reconnect is safe only through the existing generation fence and must not duplicate capture writers. Accessibility trees can independently exceed their cap even when DOM acquisition is skipped, so the error must distinguish that case. Explicit activation necessarily steals browser focus, which is why it must remain a named one-shot operation rather than an implicit retry.

## Implementation outcome

- Terminal capture acknowledgement failures retain their one-shot gap/failure truth and notify the existing generation-fenced reconnect path. The failed token is never retried; a later attachment generation persists new frames.
- Semantic query acquisition now follows one core variant policy. Plain role/name queries remain AX-only, DOM-dependent variants retain exact selected-document semantics, and both bounded acquisition failures provide narrowing guidance.
- `AllowPartial` clamps only intersecting interaction/latest-interaction natural ranges to retained capture bounds. Requested range and anchor identity remain authoritative, with explicit `PartiallyCaptured` and affected-edge warnings.
- `activate_page` is a generated, non-batchable MCP operation that explicitly foregrounds the selected or named target through the shared bounded activation authority, returns live evidence, and leaves logical selection plus immutable focus policy unchanged.
- Foundation docs and the plugin skill now describe all current contracts. The design's named `browser/action.rs` file did not exist; page request/result authority correctly remained in `browser/control.rs`.
- Root smoke tests now derive their tool count from the operation registry without a stale numeric assertion and give every CLI probe a private writable data root.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (passed outside the managed sandbox; four loopback transport tests require local socket binding)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `bun run docs:build`
- Focused capture reconnect, semantic frame-query, interaction-tail, activation, generated-schema, and scripted page-lifecycle regressions all passed before the integrated suite.

## Execution notes

- `.work/bin/work-view` is a Linux executable and cannot run on this macOS host, so item/dependency state was inspected directly from the Markdown substrate.
- The first sandboxed full test run denied four loopback socket binds with `EPERM`; the authoritative approved rerun outside that restriction passed.
- Story commits: `c915567`, `c14ad33`, `6c4d61e`, `0a9a3d7`.

## Review (2026-07-18)

**Verdict**: Approve

**Blockers**: none — obsolete preserve-focus recovery and mixed eviction/capture-tail clamping were resolved in `d619e78`

**Important**: none — activation lifecycle and transport error propagation were resolved in `d619e78`

**Nits**: none

**Rejected**: none

**Notes**: Substrate feature review at effective weight `standard`; exactly one same-harness fresh-context GPT-5.6 Sol balanced pass. The receiver adjudicated two blockers and one important finding as current-cycle work, fixed them, and verified focused regressions plus full workspace fmt/check/test/clippy. Closure used fix verification only; no second independent pass. Limitations: same-harness rather than cross-model; opt-in real-Chrome qualification was not rerun; deterministic, scripted, and integration lanes were used; concurrent unrelated MCP response edits were excluded from the reviewer’s scope.
