---
id: feature-semantic-wait-nonactionable
kind: feature
stage: done
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Semantic waits match non-actionable content

## Brief

Semantic waits cannot match non-actionable content, and the failure mode is
a bare timeout. `wait {condition: semantic}` shares `query_page`'s matching,
which returns only actionable references — so waiting for a toast, status
banner, or alert (`role: status` / `role: alert` content divs, arguably the
headline use case for a semantic wait) never matches and times out with no
hint that the role can never satisfy the query. Repro (v1.5.0 shakedown): a
`role=status` div added to the DOM with visible text; `wait semantic
{role: status}` times out while a `text` wait for the same content
satisfies immediately. Semantic waits work correctly for actionable targets
(verified with `role: button` + name).

Directions to consider: extend semantic-wait matching to the non-actionable
accessibility tree (snapshot-style, not query_page-style); or fail fast /
warn when the queried role is one the actionable matcher can never return;
and document the actionable-only scope in the wait schema description
either way.

## Simplification opportunity

If wait-side matching widens to the snapshot-style tree, the wait condition
and query_page stop sharing one matcher; keep the shared query language but
name the two match scopes explicitly rather than adding a parallel query
dialect. The wait schema description must state whichever scope ships.

## Design decisions

**Decision: widen semantic-wait matching to the full acquired accessibility
tree.** A semantic wait probe matches over every node of the selected page's
main-document snapshot — actionable or not — while `query_page` keeps its
current contract of returning only actionable references. The two surfaces
keep one query language (`SemanticQuery` + `semantic_query_matches`) and one
acquisition path; only the candidate filter differs, and that divergence is
named explicitly in the wire schema description, SPEC, and the plugin skill.

Rationale:

1. **The headline use case is a trap today.** "Wait until the toast/status/
   alert appears (or disappears)" can never succeed under actionable-only
   matching: `role=status`/`role=alert` content nodes carry no actionable
   signal, so a present-wait times out forever with zero diagnostic signal
   (v1.5.0 shakedown repro in the Brief).
2. **Widening is structurally leak-free.** The wait result carries
   `WaitProbe::Semantic { matched, outcome, match_count,
   relaxed_match_candidates }` — no `SemanticMatch`, no `NodeReference`. The
   "no references leaked" invariant (SPEC: "never carries an actionable
   reference; callers follow with `query_page` before acting") is satisfied
   by the result shape itself, not by the match filter, so widening the
   filter cannot violate it.
3. **Zero extra acquisition cost.** Every semantic probe already calls
   `Accessibility.getFullAXTree` and decodes non-actionable nodes into
   `snapshot.nodes` with role/name/value (`snapshot.rs` `visit`, ~line 2338;
   `reference` is `Some` only when actionable). DOM semantic metadata
   (rendered text, labels, test ids) is keyed by `node_by_backend`, which is
   populated for every backend-bearing node regardless of actionability
   (~line 2310), so label/text/test-id waits widen for free too.
4. **The refusal alternative is unsound, not just weaker.** Actionability is
   not role-determined: a node is actionable when its role is in
   `ACTIONABLE_ROLES` *or* it carries a focusable/editable/clickable signal
   (`snapshot.rs` ~line 2329). A focusable `role=status` div is actionable
   today, so a static "this role can never match" refusal would reject
   queries that can legitimately succeed, and a warning would still leave
   the headline use case unserved. Current Contract Discipline prefers one
   correct behavior over a documented trap plus a diagnostic band-aid.

**Presence semantics under the chosen scope.** `present` is satisfied when
at least one node in the full acquired accessibility tree of the selected
page's main document matches; `absent` is satisfied when zero nodes in that
same tree match. One tree for both directions keeps presence/absence
coherent: a dismissed toast (removed or `display:none`/`aria-hidden`, which
Chrome drops from the AX tree as ignored) satisfies `absent`; a visible
status region satisfies `present`. Note the deliberate behavior change:
`absent` for a role that exists only non-actionably previously satisfied
immediately (a false positive); it now correctly keeps waiting.

**Probe outcome and count under the chosen scope.** The probe is count-only
(no match set, no match limit), so outcome derivation is 0 → `no_match`,
1 → `unique`, ≥2 → `ambiguous`; `truncated` no longer occurs in wait probes
(it remains a `query_page` outcome — the wire enum is unchanged).
`match_count` reports the true count (saturating u32) instead of
matches-retained-plus-omitted. `relaxed_match_candidates` keeps its current
contract — reported only on an unmatched present-wait whose query used an
exact matcher — computed over the widened node set with the same
`MAX_SEMANTIC_RELAXED_CANDIDATES` cap.

**Unchanged:** the 100 ms semantic poll floor, stale-capture retry at the
next poll, explicit failure on bounded snapshot-acquisition limits
(`omitted_node_count != 0`), the DOM-semantics-required guard, `query_page`
results and interaction paths, and every non-semantic wait condition.

## Architectural choice

Keep one matcher and one acquisition path; add a second, reference-free
*consumer* of the snapshot registry instead of a mode on `query_page`:

- `SnapshotRegistry` gains a `probe_presence` method that runs the same
  active-snapshot validation and `semantic_query_matches` evaluation as
  `query`, but scans all nodes (no `node.reference?` filter) and returns a
  count-shaped result with no references. `query` is untouched externally.
- The wait path stops routing through `query_page` +
  `QueryPageRequest`/`DEFAULT_SEMANTIC_MATCH_LIMIT` (which exists to bound
  *returned reference sets*, a concern the wait never had) and calls the
  probe directly after the same `capture_snapshot_for_frame` acquisition.
- No public knob, no dual mode, no parallel query dialect: `SemanticQuery`
  stays the single query language; the scope divergence lives in two
  internal call sites and is documented at the contract surfaces
  (`WaitConditionWire::Semantic` doc comment → generated wire schema,
  `docs/SPEC.md`, `plugin/skills/krometrail/SKILL.md`).
- `check-wire-enum-schemas.sh` is unaffected: no new wire enums are
  introduced (the probe result is an internal non-serialized struct), and
  the schema change is doc-comment text on an existing wire shape.

## Implementation Units

### Unit 1 — Registry presence probe

**File:** `crates/krometrail-cdp/src/control/snapshot.rs`

Add an internal probe result and registry method:

```rust
#[derive(Debug)]
pub(super) struct SemanticPresenceProbe {
    pub outcome: SemanticQueryOutcome,
    pub match_count: u32,
    pub relaxed_match_candidates: Option<RelaxedMatchCandidates>,
}

impl SnapshotRegistry {
    pub(super) fn probe_presence(
        &self,
        bound: &BoundTarget,
        query: &SemanticQuery,
        snapshot: &PageSnapshot,
    ) -> Result<SemanticPresenceProbe>
}
```

Notes:
- Extract the shared query prelude from `SnapshotRegistry::query` into a
  private helper — active-snapshot resolution + staleness filter, the
  DOM-semantics-required guard, and the `omitted_node_count != 0` node-limit
  error — e.g. `fn active_for_query(&self, bound: &BoundTarget, snapshot:
  &PageSnapshot, requires_dom_semantics: bool) -> Result<&ActiveSnapshot>`.
  `query` keeps its additional scope-reference validation; the probe has no
  descendant scope (wait conditions carry only `query` + `presence`).
- The probe scans `snapshot.nodes` with the existing
  `semantic_query_matches` evaluation (same closure shape as `query`) but
  **without** the `node.reference?` filter. Count all matches; derive
  outcome 0/1/many → `NoMatch`/`Unique`/`Ambiguous`.
- When the count is zero and `query.relaxed_to_contains()` is `Some`, scan
  the same widened node set for relaxed candidates, capped at
  `MAX_SEMANTIC_RELAXED_CANDIDATES`, via `RelaxedMatchCandidates::new`.
- `query()` behavior must be byte-identical for callers: actionable-only
  matches, reference-bearing relaxed-candidate scan, same errors.

Acceptance criteria:
- Unit tests (in-file, alongside the existing registry tests) cover:
  outcome derivation for 0/1/many matches including a non-actionable
  `status` node; relaxed-candidate reporting over non-actionable nodes;
  DOM-semantics guard and node-limit error propagate identically to
  `query`; `query` still excludes the non-actionable node the probe
  matches.

### Unit 2 — Wait path rewiring

**Files:** `crates/krometrail-cdp/src/control/snapshot.rs`,
`crates/krometrail-cdp/src/control/wait.rs`

Add the acquisition+probe entry point on `PageControl` (mirrors
`query_page` minus the registry `query` call):

```rust
impl PageControl {
    pub(super) async fn probe_semantic_presence(
        &mut self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        query: &SemanticQuery,
        started_at: krometrail_core::SessionTime,
    ) -> Result<SemanticPresenceProbe>
}
```

calling `capture_snapshot_for_frame(transport, bound, started_at,
query.requires_dom_semantics(), false, None, None)` then
`self.snapshots.probe_presence(bound, query, &snapshot)`.

Rewrite `PageControl::probe_semantic` in `wait.rs`:

```rust
async fn probe_semantic(
    &mut self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    query: &SemanticQuery,
    presence: WaitPresence,
) -> Result<WaitProbe>
```

- `matched = (presence == WaitPresence::Present) != (probe.outcome ==
  SemanticQueryOutcome::NoMatch)` (unchanged formula).
- Forward `relaxed_match_candidates` only when `!matched && presence ==
  Present` (unchanged filter).
- Drop the `QueryPageRequest` construction and the
  `DEFAULT_SEMANTIC_MATCH_LIMIT` / `QueryPageRequest` imports from
  `wait.rs`.
- The `StaleReference` swallow-and-retry in `execute_wait` (~line 95) stays;
  the probe path surfaces the same stale errors from
  `capture_snapshot_for_frame`.

Acceptance criteria:
- `WaitProbe::Semantic` wire shape unchanged (no core type edits needed).
- Existing scripted actionable-wait tests in
  `crates/krometrail-cdp/tests/waits_and_batches.rs` pass unmodified
  (`scripted_semantic_wait_present_satisfies_in_one_poll`,
  `..._absent_satisfies_in_one_poll`,
  `..._timeout_carries_relaxed_match_candidates`).

### Unit 3 — Contract text

**Files:** `crates/krometrail-core/src/browser/wait.rs`, `docs/SPEC.md`,
`plugin/skills/krometrail/SKILL.md`, generated `docs/public/llms-full.txt`

- `WaitConditionWire::Semantic` doc comment (this is the generated wire
  schema description) — replace the current three lines with text stating:
  same query language as `query_page`; matched over the **full
  accessibility tree** of the selected page's main document, including
  non-actionable content such as `status`/`alert`/heading/text nodes, while
  `query_page` returns actionable references only; `present` = at least one
  matching node, `absent` = none; result reports outcome and match count
  and never carries a reference; 100 ms poll floor.
- `docs/SPEC.md` Waiting section (~lines 305–315): extend the semantic-wait
  bullet and paragraph with the full-tree match scope and the explicit
  named divergence from structured queries ("structured queries return
  actionable references; semantic waits observe the full tree and return
  none"). Do not touch line ~136 ("frame-scoped semantic queries remain
  actionable-reference discovery") — `query_page` is unchanged.
- `plugin/skills/krometrail/SKILL.md` (~lines 174–181): update the semantic
  wait paragraph to say it also waits on non-actionable content (toasts,
  status regions, alerts) and remains reference-free.
- Regenerate `docs/public/llms-full.txt` with `bun run docs:build` (never
  edit it directly).

Acceptance criteria:
- Generated wait schema description names the full-tree scope and the
  query_page divergence; `bash scripts/check-wire-enum-schemas.sh` passes;
  no serde shape change in `WaitConditionWire`.

### Unit 4 — Regression and qualification tests

**Files:** `crates/krometrail-cdp/tests/waits_and_batches.rs`,
`tests/fixtures/browser/waits-and-batches/index.html`

Scripted (deterministic double), following the existing
`semantic_ax_tree` fixture pattern — extend it (or add a sibling helper)
with a non-actionable node: `role=status`, a `backendDOMNodeId`, a name or
rendered text, and **no** focusable/editable/clickable property:

- `scripted_semantic_wait_matches_nonactionable_status_role` — present-wait
  `role=status` satisfies in one poll; probe `outcome: Unique,
  match_count: 1`.
- `scripted_semantic_wait_absent_holds_while_status_persists` — absent-wait
  against the tree containing the status node uses
  `wait_for_with_timeout` with a short timeout and reports `TimedOut` with
  a final unmatched probe (pins that `absent` is no longer a false
  positive).
- `scripted_semantic_wait_absent_satisfies_when_status_removed` —
  absent-wait against the tree without the node satisfies.
- Divergence pin: in the registry unit tests (Unit 1), the same snapshot
  yields `query_page` → `NoMatch` for `role=status` while `probe_presence`
  counts 1. This is the executable statement of the two named scopes.

Real-Chrome opt-in qualification (layered-cdp-qualification ladder):

- Extend `tests/fixtures/browser/waits-and-batches/index.html`: the
  existing "Start delayed states" script additionally populates (and later
  clears) a `<div role="status">` region, alongside the current delayed
  text/button states.
- Extend `opt_in_real_chrome_qualifies_semantic_wait_present_and_absent`
  (or add a sibling test behind the same `KROMETRAIL_REAL_CHROME_TESTS`
  guard): present-wait `role=status` satisfies after the delayed script
  fires; absent-wait satisfies after the region clears.

Acceptance criteria:
- All new scripted tests pass without real Chrome; existing wait tests
  unmodified and green; full gate passes (`cargo fmt --all -- --check`,
  `bash scripts/check-wire-enum-schemas.sh`, `cargo check/test/clippy
  --workspace --all-targets --locked`).

## Implementation Order

1. Unit 1 — registry `probe_presence` + shared prelude extraction (with
   its in-file unit tests, including the divergence pin).
2. Unit 2 — `PageControl::probe_semantic_presence` + `wait.rs` rewiring.
3. Unit 4 (scripted portion) — integration regression tests.
4. Unit 3 — schema doc comment, SPEC, skill doc, llms-full regeneration.
5. Unit 4 (qualification portion) — fixture extension + real-Chrome opt-in
   test.

Units 1–3 are one coherent stride; no child stories are warranted at this
size.

## Simplification

- The Brief's simplification opportunity is resolved by construction: one
  query language, one matcher (`semantic_query_matches`), one acquisition
  path (`capture_snapshot_for_frame`); the only divergence is the candidate
  filter at two internal call sites, and it is named at every contract
  surface rather than hidden.
- The wait path sheds `query_page` coupling it never needed:
  `QueryPageRequest` construction, `DEFAULT_SEMANTIC_MATCH_LIMIT`, and
  match-set truncation existed to bound returned reference sets, which the
  wait discards. The count-only probe removes that dead weight.
- No compatibility shim: the previous actionable-only wait behavior is
  replaced outright (Current Contract Discipline); the old `absent`
  false-positive and `truncated` probe outcome are not preserved behind a
  flag.

## Testing

Regression (deterministic, no real browser):
- Present-wait `role=status` against a fixture tree containing a
  non-actionable status node with visible text: satisfied in one poll
  (the Brief's live repro, inverted into a green test).
- Absent-wait for the same node: times out while the node persists;
  satisfies once the tree no longer contains it (absence = zero matches in
  the full tree).
- Actionable waits unchanged: existing present/absent/relaxed-candidate
  scripted tests pass unmodified.
- Scope-divergence pin: same snapshot, `query_page` reports `no_match` for
  `role=status` (still actionable-reference discovery) while the wait
  probe counts one match.
- Registry unit tests: outcome derivation (0/1/many), relaxed-candidate cap
  over the widened set, DOM-semantics guard, snapshot node-limit failure.

Qualification (opt-in real Chrome):
- Delayed `role="status"` region in the waits-and-batches fixture; wait
  present satisfies after the delay, wait absent satisfies after clearing.

Gate: `cargo fmt --all -- --check`, `bash
scripts/check-wire-enum-schemas.sh`, `cargo check/test/clippy --workspace
--all-targets --locked`.

## Risks

- **Deliberate behavior change on `absent`.** An absent-wait for content
  that exists only non-actionably previously satisfied immediately; it now
  waits (and may time out). This is the corrected semantics, but any caller
  who relied on the false positive sees a change. Mitigation: named in
  SPEC, the wire schema description, and the skill doc; no supported
  third-party consumers exist.
- **Probe outcome drift for large match sets.** `truncated` no longer
  occurs in wait probes and `match_count` is now the true count rather than
  retained+omitted. Wire enum unchanged; drift documented in the schema
  description. Low impact: agents branch on `matched`, not on outcome
  granularity.
- **Hidden-but-present AX nodes.** A node flagged `hidden` can, in rare
  cases, remain in the AX tree and now satisfy `present`. Accepted:
  presence means "present in the accessibility tree", and Chrome drops
  `aria-hidden`/`display:none` content as ignored in the common
  toast-dismissal path. Called out in SPEC wording ("full acquired
  accessibility tree").
- **Refactor regression in `query`.** Extracting the shared prelude must
  not alter `query_page` behavior (actionable filter, reference-bearing
  relaxed-candidate scan, error taxonomy). Guarded by the existing
  extensive `query` unit/integration tests plus the new divergence pin.
- **Cost.** None expected: acquisition is unchanged; matching already
  iterates the bounded snapshot (`MAX_SNAPSHOT_NODES`), the probe merely
  stops skipping non-reference nodes.

## Implementation notes

- Execution capability: inline implementation; the feature is one cohesive CDP/core contract change with a shared snapshot seam and one owning test surface.
- Review weight: standard default; the item remains at `stage: implementing` per the caller's instruction and was not committed or advanced to review.
- Files changed: `.work/active/features/feature-semantic-wait-nonactionable.md`, `crates/krometrail-cdp/src/control/snapshot.rs`, `crates/krometrail-cdp/src/control/wait.rs`, `crates/krometrail-cdp/tests/waits_and_batches.rs`, `crates/krometrail-core/src/browser/wait.rs`, `docs/SPEC.md`, `plugin/skills/krometrail/SKILL.md`, `tests/fixtures/browser/waits-and-batches/index.html`.
- Tests added: registry probe coverage for 0/1/many full-tree outcomes, actionable/query scope divergence, relaxed candidates over non-actionable nodes with cap saturation, DOM-semantic guard, and node-limit parity; scripted present/absent status waits; opt-in real-Chrome delayed status qualification.
- Simplification: extracted the shared active-snapshot query prelude and removed wait-side `QueryPageRequest`, `DEFAULT_SEMANTIC_MATCH_LIMIT`, retained-match truncation, and result-shape coupling.
- Discrepancies from design: generated `docs/public/llms-full.txt` was regenerated successfully with `bun run docs:build` and had no content diff; no other deviations.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; `bash scripts/check-wire-enum-schemas.sh`; `cargo check --workspace --all-targets --locked`; `cargo test --workspace --all-targets --locked`; and `cargo clippy --workspace --all-targets --locked -- -D warnings` all passed using `CARGO_TARGET_DIR=/tmp/krometrail-target`, serialized build jobs for the disk budget, and approved local transport permissions for the environment-sensitive tests.
