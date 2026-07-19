---
id: runtime-observation-hardening
kind: feature
stage: implementing
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Harden live browser observation under real public-site load

## Brief

Resolve the manual cross-surface findings from Krometrail 1.1.1 as one cohesive patch: restore reliable responsive viewport acknowledgement, keep screencast acknowledgement healthy during frame-heavy navigation, expose privacy-bounded viewport and CDP facts in failure diagnostics, rank compact snapshots around actionable content rather than raw preorder, and add an explicit interaction-only snapshot projection for callers that need the smallest targeting surface.

The work preserves the stable canonical snapshot, target-scoped viewport lifecycle, immediate-ack/bounded-handoff capture contract, and existing compact/full/omit meanings. New presentation behavior must derive from canonical acquisition and must not weaken reference authority, loss accounting, or diagnostic privacy.

## Simplification opportunity

Consolidate viewport acknowledgement facts in the existing observation result instead of reconstructing them at error/log boundaries, and centralize snapshot node selection so compact and interaction-only projections share one deterministic ranking implementation rather than separate traversal paths.

## Design decisions

- **Desktop viewport authority**: extend the existing privacy-bounded runtime observation with `window.innerWidth` and `window.innerHeight`; use those values for exact desktop override acknowledgement while retaining CDP `cssVisualViewport` as the separately reported content area. Chrome 150 reports both CDP layout and visual widths as scrollbar-reduced on overflowing pages, while the runtime layout width remains the requested geometry.
- **Acknowledgement deadline**: raise the production screencast acknowledgement deadline from 250 ms to 1 second, matching the qualified transport maximum. Keep one-shot acknowledgement, ack-before-handoff ordering, and terminal failure beyond the deadline; never retry an ambiguous acknowledgement token.
- **Snapshot contract**: add `interaction_only` only to `response.snapshot`, not to page-state projection or canonical acquisition. Compact and interaction-only share one deterministic selector; full, legacy, omit, registry installation, and reference authority remain unchanged.
- **Action ranking**: prioritize focused actions, then editable actions, other non-link actions, and links, with canonical preorder as the tie-breaker. Admit each action together with its complete missing ancestor path or not at all.
- **Diagnostics privacy**: log only expected/observed numeric geometry, mismatch flags, command/stage categories, bounded counters, timeouts, and opaque session/target/generation identities. Never log raw CDP payloads, acknowledgement tokens, page content, titles, URLs, or runtime expressions.

## Architectural choice

Extend the existing viewport observation, capture configuration, and MCP response projector rather than adding retry layers, alternate capture tasks, or a second snapshot model. This preserves the lifecycle-complete browser override, bounded-loss accounting, and canonical-result projection patterns while correcting the authorities and defaults that failed under public-site load.

## Implementation units

### Unit 1: Truthful viewport acknowledgement and diagnostic facts

**Story**: `runtime-observation-hardening-viewport-diagnostics`

**Files**: `crates/krometrail-cdp/src/control/viewport.rs`, `crates/krometrail-cdp/tests/verified_interactions.rs`

```rust
fn decode_effective_viewport(
    layout: &Value,
    runtime: &Value,
    declared: Option<ViewportMetrics>,
    target_id: TargetId,
) -> Result<EffectiveViewport>;
```

The existing runtime projection gains layout width/height. Desktop acknowledgement compares declared metrics with those values; mobile continues to validate the visual viewport. A structured failure event records privacy-safe expected/observed facts and individual mismatch flags before returning the stable `target_failed` error.

**Acceptance criteria**:
- [ ] An overflowing 390px desktop page with a 384px visual content area succeeds and reports layout 390 / visual 384.
- [ ] A true layout, DPR, or touch mismatch still fails and logs the exact bounded mismatch facts.
- [ ] The public-site reproduction against `https://krometrail.dev/` succeeds in real Chrome.

### Unit 2: Frame-heavy acknowledgement resilience

**Story**: `runtime-observation-hardening-capture-acknowledgements`

**Files**: `crates/krometrail-cdp/src/capture/mod.rs`, `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/capture/tests.rs`, relevant smoke fixtures

```rust
impl Default for CaptureConfig {
    fn default() -> Self; // ack_timeout = 1 second
}
```

The frame reader remains synchronous and immediate, but the fatal deadline aligns with the qualified transport envelope. Failure logging adds reason, elapsed/deadline, lifecycle identity, and bounded pipeline counters without raw transport output.

**Acceptance criteria**:
- [ ] A deterministic acknowledgement delayed beyond 250 ms but below 1 second succeeds before handoff.
- [ ] A configured acknowledgement exceeding its deadline remains a terminal acknowledgement failure with one explicit gap.
- [ ] Frame-heavy/nested-frame real-Chrome navigation remains capturing with received and acknowledged counts equal for the observed interval.

### Unit 3: Action-centric and interaction-only snapshot projection

**Story**: `runtime-observation-hardening-snapshot-projections`

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`, `plugin/skills/krometrail/SKILL.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`

```rust
enum SnapshotResponseDetail { Legacy, Full, Compact, InteractionOnly, Omit }
enum SnapshotProjection { Compact, InteractionOnly }
fn project_snapshot(
    snapshot: PageSnapshot,
    projection: SnapshotProjection,
) -> Result<PageSnapshot, ResponseInvariantError>;
```

`page_state` retains the existing detail enum. The shared projector ranks actionable nodes, atomically admits their missing ancestor closure within the 48-node / 12-KiB budgets, emits canonical preorder, and optionally fills compact mode with contextual preorder nodes. Interaction-only never fills unrelated static content.

**Acceptance criteria**:
- [ ] Early informational links cannot displace a later editable textbox in compact mode.
- [ ] Interaction-only returns actions plus complete ancestry, exact references, and correct omission counts.
- [ ] Full, legacy, compact, interaction-only, omit, node/byte budgets, and invalid page-state projection are covered at the stable MCP boundary.

## Implementation order

1. Correct viewport authority and add its public-site regression evidence.
2. Align the capture acknowledgement deadline and add bounded failure facts.
3. Land the shared snapshot selector and additive wire projection.
4. Update standing docs and the plugin skill, regenerate derived docs, then run integrated verification and the manual reproductions.

## Simplification

- Replace the existing two-pass preorder-biased compactor with one ranked closure selector.
- Retain one viewport observation command pair and one capture reader; do not add polling, retries, or parallel authorities.

## Testing

- Unit regressions cover Chrome's scrollbar-reduced CDP metrics, true viewport mismatches, acknowledgement deadline behavior, ranked action selection, ancestor atomicity, budgets, and omission accounting.
- Real-Chrome qualification covers the Krometrail docs viewport reproduction and frame-heavy nested navigation.
- Workspace tests and strict clippy remain required; optional agile release gate scans are explicitly skipped by user request.

## Risks

- Runtime and CDP geometry could diverge for page zoom or mobile emulation; desktop-only authority selection and existing mobile validation bound that risk.
- A longer acknowledgement deadline must not hide transport failure; it remains finite, terminal, and aligned with the already-qualified maximum.
- Ranking can starve contextual text; compact retains a context-fill phase, while interaction-only is explicitly opt-in.
