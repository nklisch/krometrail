---
id: feature-container-text-generic-ancestors
kind: feature
stage: review
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# container_text works on generic-div markup

## Brief

`container_text` role queries silently fail on generic-div markup. The
qualifier only consults ancestors whose role is in `LOCAL_CONTAINER_ROLES`
(`listitem, row, cell, gridcell, group, article, region, label, labeltext` —
`crates/krometrail-cdp/src/control/snapshot.rs:60`), so a checkbox inside
`<div class="row"><input type="checkbox"><span>Buy milk</span></div>` never
qualifies: the div is role `generic`, the walk finds no eligible container,
and the query returns bare `no_match` even in `contains` mode. Repro during
the v1.5.0 shakedown: the skill doc's own example shape
(`role: checkbox, container_text: "Buy milk"`) returns `no_match` while a
plain `role: checkbox` query finds the node. Most real apps build rows from
styled divs (flexbox/Tailwind), so the ergonomic entry point misses exactly
the markup it was designed for, and the failure gives no hint that the
control exists but no eligible container ancestor was found.

Directions to consider: surface an explicit "matched controls exist but no
eligible container ancestor" outcome or hint; or extend eligibility with a
bounded generic-ancestor fallback; and align the skill doc's "nearest
matching ancestor's rendered text" wording with the actual allowlist rule.

## Simplification opportunity

The fix may let the plugin skill doc drop its implicit "works on any
markup" promise or, conversely, keep one matching rule with no special
cases. Whichever eligibility rule wins, the doc wording and the
implementation must converge to a single stated contract.

## Design decisions

- **Primary direction — make generic-div markup work** (bounded
  generic-ancestor eligibility), not diagnosis-only. The skill doc's own
  example is the repro; most real apps build rows from styled divs, so a
  diagnosis-only fix would leave the ergonomic entry point unusable on the
  dominant markup style with no working alternative.
- **Bounding rule — collapsed-rendered-text byte cap, not depth**: a
  generic ancestor is eligible only while its whitespace-collapsed rendered
  text is at most `MAX_GENERIC_CONTAINER_TEXT_BYTES` (1024, matching
  `MAX_SEMANTIC_QUERY_TEXT_BYTES`'s scale). Rendered text is monotonically
  non-decreasing up the ancestor chain, so a page-root div disqualifies
  itself on any real page and every ancestor above the first over-cap
  generic is also over cap — no depth heuristics or extra walk state needed.
- **Authority semantics stay asymmetric**: an allowlisted semantic
  container (`LOCAL_CONTAINER_ROLES`) remains the sole authority the moment
  the walk reaches it — its text decides the match, walk ends (current
  behavior preserved). Generic ancestors are opportunistic: a bounded
  generic that matches qualifies the control; a bounded generic that does
  not match is transparent and the walk continues. Rationale: semantic
  container roles declare identity boundaries; styling divs do not, so no
  single generic div is authoritative, and column-wrapper markup
  (`row > col > input`) must not fail at an inner textless wrapper.
- **Generic role set is one registry**: `GENERIC_CONTAINER_ROLES =
  ["generic", "none", "presentation"]`, compared case-insensitively like
  the existing role checks. Structural web-area roles are not in it, so
  document/rootwebarea text still never qualifies anything (unchanged).
- **Secondary diagnosability ships in the same feature**: a `no_match` on a
  container-qualified role query additionally reports
  `uncontained_match_candidates` — a bounded count of nodes the same query
  would match with the container qualifier dropped. This is the honest
  signal for the residual failure ("the controls exist; the container
  qualification is what failed") and follows the established
  `relaxed_match_candidates` precedent for explaining an empty result.
- **Reuse `RelaxedMatchCandidates` for the new field, no rename**: both
  fields count matches under a relaxation of the failed query (exact →
  contains; container qualifier dropped). Generalize the struct's doc
  comment instead of renaming — avoids schema/type churn for no contract
  gain.
- **Accepted degradation, not prevented**: on a page whose repeated-row
  wrapper (or a tiny page's root div) is under the cap, a `contains`
  container query can qualify sibling rows' controls. The result degrades
  to `ambiguous`, never to a silently wrong reference — the existing
  narrow-until-unique invariant is the safety net. Doc guidance keeps
  `exact` mode as the default example and points to `scope` for narrowing.
- **SPEC states the bound exists without hardcoding the number**, matching
  how the relaxed-candidate cap is already worded ("a declared candidate
  limit").
- **No child stories**: single-stride, tightly cohesive change (one walk
  function, one result field, doc convergence). The feature is the
  implementation unit.

## Architectural choice

Three directions were considered:

1. **Diagnosis-only** — keep the allowlist, add an explicit
   "no eligible container ancestor" hint on `no_match`. Honest but leaves
   the primary use case broken on the markup it was designed for; the agent
   learns why it failed and still has no way to express the query.
2. **Unbounded generic eligibility** — treat any ancestor's rendered text
   as a match scope. Rejected outright: rendered text propagates to
   page-level divs, so every control would qualify against any page text in
   `contains` mode; this is exactly why the allowlist existed.
3. **Bounded generic-ancestor eligibility + uncontained-candidate
   diagnostic** (chosen) — semantic containers keep their authority;
   generic ancestors become eligible only while their collapsed rendered
   text stays under a declared byte cap; the residual `no_match` reports
   how many controls matched everything but the container qualifier.

Direction 3 is chosen because the cap converts the false-positive risk
into a self-limiting property (text monotonicity means page-scale scopes
disqualify themselves), preserves all current allowlist behavior
unchanged, and pairs the working path with an honest diagnostic for the
cases the cap still excludes. One current contract, no compatibility
shims: `nearest_container_text_matches` is rewritten in place, the one
existing test that encoded "small generic wrapper text never qualifies"
is re-encoded to the new contract, and SPEC + plugin doc are updated in
the same stride.

## Implementation Units

### Unit 1: Core contract — cap constant, query relaxation, result diagnostic

**File**: `crates/krometrail-core/src/browser/observation.rs` (plus re-exports in `crates/krometrail-core/src/lib.rs`)

```rust
/// The declared bound on generic-ancestor container eligibility. A generic-role ancestor may
/// qualify a container-text query only while its whitespace-collapsed rendered text fits this
/// many UTF-8 bytes; page-scale containers exceed it and never qualify.
pub const MAX_GENERIC_CONTAINER_TEXT_BYTES: usize = 1_024;

/// Collapsed byte length of rendered text under the same normalization semantic text
/// matching uses (whitespace collapsing, invisible-format stripping, private-use glyphs as
/// separators), without case folding.
pub fn collapsed_semantic_text_bytes(value: &str) -> usize {
    normalize_semantic_text(value, true).len()
}

impl SemanticQuery {
    /// The same role query with the container qualifier dropped.
    ///
    /// `None` for non-role queries and role queries without `container_text`; this is how a
    /// `no_match` result reports that matching controls exist outside any qualifying container.
    pub fn without_container_text(&self) -> Option<Self>;
}

pub struct QueryPageResult {
    // ...existing fields...
    /// Present only on `no_match` of a container-qualified role query when the same query with
    /// the container qualifier dropped would have matched at least one node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncontained_match_candidates: Option<RelaxedMatchCandidates>,
}

impl QueryPageResult {
    pub fn with_no_match_diagnostics(
        context: ObservationContext,
        generation: SnapshotGeneration,
        matches: Vec<SemanticMatch>,
        max_matches: u16,
        relaxed_match_candidates: Option<RelaxedMatchCandidates>,
        uncontained_match_candidates: Option<RelaxedMatchCandidates>,
    ) -> Result<Self>;
}
```

**Implementation Notes**:
- Place `MAX_GENERIC_CONTAINER_TEXT_BYTES` beside the other semantic limits
  (observation.rs ~line 18-23) and re-export from `lib.rs` next to
  `MAX_SEMANTIC_QUERY_TEXT_BYTES`.
- `collapsed_semantic_text_bytes` wraps the existing private
  `normalize_semantic_text` (observation.rs ~line 631) with
  `case_sensitive: true` so the measure is not distorted by lowercase
  expansion.
- Replace `with_relaxed_candidates` with `with_no_match_diagnostics`
  directly (Current Contract Discipline — no deprecated alias). `new()`
  stays as the no-diagnostics convenience. Both diagnostic fields are
  filtered to `outcome == NoMatch` and `count > 0`, same as today's
  relaxed-candidate gating (~line 996).
- Generalize the `RelaxedMatchCandidates` doc comment: it counts how many
  nodes a specific relaxation of a `no_match` query would have reached
  (exact matchers relaxed to `contains`, or the container qualifier
  dropped), capped at `MAX_SEMANTIC_RELAXED_CANDIDATES` with `saturated`
  marking the cap.
- `without_container_text` returns `Some(Role { role, name, container_text: None })`
  only when `container_text.is_some()`; `None` for every other shape.

**Acceptance Criteria**:
- [ ] `SemanticQuery::without_container_text` returns the stripped role
      query for container-qualified queries and `None` otherwise.
- [ ] `QueryPageResult` serialization omits `uncontained_match_candidates`
      when `None`; the field survives a serde round-trip when present.
- [ ] `with_no_match_diagnostics` suppresses both diagnostics on any
      non-`NoMatch` outcome and on zero counts.
- [ ] `collapsed_semantic_text_bytes` collapses whitespace runs and
      ignores invisible format characters consistently with
      `SemanticTextMatch::matches`.
- [ ] `bash scripts/check-wire-enum-schemas.sh` passes (no new bare wire
      enums; the added field is an optional struct member).

---

### Unit 2: CDP matcher — bounded generic-ancestor walk and uncontained scan

**File**: `crates/krometrail-cdp/src/control/snapshot.rs`

```rust
const GENERIC_CONTAINER_ROLES: &[&str] = &["generic", "none", "presentation"];

fn is_generic_container_role(role: &str) -> bool;

fn nearest_container_text_matches(
    node: SnapshotNodeId,
    expected: &krometrail_core::SemanticTextMatch,
    parents: &HashMap<SnapshotNodeId, Option<SnapshotNodeId>>,
    semantic: &HashMap<SnapshotNodeId, SemanticNodeMetadata>,
    nodes: &[SnapshotNode],
) -> bool; // signature unchanged; walk semantics replaced
```

New walk (replaces the body at ~line 870):

```rust
let mut current = parents.get(&node).copied().flatten();
while let Some(ancestor) = current {
    let Some(ancestor_node) = nodes.iter().find(|candidate| candidate.id == ancestor) else {
        return false;
    };
    if is_local_container_role(&ancestor_node.role) {
        // A semantic container declares an identity boundary: the nearest one is the sole
        // authority for the query, exactly as before.
        return semantic
            .get(&ancestor)
            .is_some_and(|metadata| expected.matches(&metadata.rendered_text));
    }
    if is_generic_container_role(&ancestor_node.role)
        && let Some(metadata) = semantic.get(&ancestor)
        && krometrail_core::collapsed_semantic_text_bytes(&metadata.rendered_text)
            <= krometrail_core::MAX_GENERIC_CONTAINER_TEXT_BYTES
        && expected.matches(&metadata.rendered_text)
    {
        // Styling divs do not declare identity boundaries, so a bounded generic ancestor
        // qualifies opportunistically and a non-matching one stays transparent. Rendered
        // text only grows upward, so every generic above the first over-cap one is also
        // over cap: page-scale wrappers can never qualify a control.
        return true;
    }
    current = parents.get(&ancestor).copied().flatten();
}
false
```

In `SnapshotRegistry::query` (~line 647), beside the existing relaxed
scan on an empty result:

```rust
let uncontained_match_candidates = if matches.is_empty() {
    request.query.without_container_text().map(|stripped| {
        let limit = usize::from(krometrail_core::MAX_SEMANTIC_RELAXED_CANDIDATES);
        let count = snapshot
            .nodes
            .iter()
            .filter(|node| node.reference.is_some() && in_scope(node))
            .filter(|node| evaluate(&stripped, node))
            .take(limit)
            .count();
        krometrail_core::RelaxedMatchCandidates::new(count)
    })
} else {
    None
};
```

and finish with `QueryPageResult::with_no_match_diagnostics(...)`.

**Implementation Notes**:
- `is_generic_container_role` mirrors `is_local_container_role`
  (`eq_ignore_ascii_case` over the registry). Keep both registries
  adjacent near `LOCAL_CONTAINER_ROLES` (~line 60).
- Structural web-area roles need no explicit check: they are in neither
  registry, so the walk passes through them and terminates at the root
  with `false`, as today.
- Do NOT return `false` on an over-cap generic: continue the walk so a
  higher allowlisted container can still decide the match (preserves
  current behavior for `listitem > huge generic > control` markup).
  Monotonicity makes this a plain `continue` — no state.
- The uncontained scan runs only on an empty result over the
  already-bounded snapshot nodes with the existing candidate cap — same
  cost profile as the relaxed scan. Both diagnostics may appear together
  on one `no_match`; they answer different questions.
- The stripped query (`container_text: None`) no longer requires DOM
  semantics, but the original did, so the active snapshot already carries
  them; no acquisition-mode edge case.

**Acceptance Criteria**:
- [ ] Shakedown repro passes: `role: checkbox, container_text: {"Buy milk",
      exact}` matches the checkbox whose only text-bearing ancestor is a
      generic div containing "Buy milk".
- [ ] Column markup passes: a textless generic wrapper between the control
      and the text-bearing generic row is transparent.
- [ ] A control whose only generic ancestors exceed the cap returns
      `no_match` with `uncontained_match_candidates` reporting the
      role-matching controls.
- [ ] Nearest allowlisted container remains sole authority: a `listitem`
      whose text does not match still fails the query even when a higher
      ancestor's text would match.
- [ ] Root/document-level page text still never qualifies a control
      (structural roles remain ineligible).
- [ ] `uncontained_match_candidates` is absent when the query matched, when
      the query had no `container_text`, and when the stripped query also
      matches nothing.

---

### Unit 3: Contract docs converge — SPEC, plugin skill, regenerated projection

**Files**:
- `docs/SPEC.md` (~lines 176-186)
- `plugin/skills/krometrail/SKILL.md` (~lines 131-137)
- `plugin/skills/krometrail/references/browser-contexts.md` (~line 22-24)
- `docs/public/llms-full.txt` (regenerated only, via `bun run docs:build`)

**Implementation Notes**:
- SPEC: replace "text rendered within its nearest matching ancestor
  container; this bounded relationship never falls back to spatial
  proximity or unrelated page text" with the two-tier rule: explicit
  semantic container roles are authoritative at the nearest occurrence; on
  markup without one, a generic ancestor qualifies only while its collapsed
  rendered text stays within a declared container-scope bound, so
  page-scale containers never qualify and the relationship still never
  falls back to spatial proximity. In the no-match reporting sentence
  cluster, add: a `no_match` for a container-qualified role query
  additionally reports how many nodes the same query would match with the
  container qualifier dropped, under the same declared candidate limit and
  saturation reporting.
- SKILL.md: rewrite the container_text paragraph ("Krometrail qualifies
  the control against the nearest matching ancestor's rendered text...")
  to state the real contract: nearest semantic container (list item, row,
  cell, group, article, region, label) decides when present; otherwise a
  bounded generic ancestor such as a styled row/card div qualifies while
  its rendered text stays small, and page-level wrappers never qualify.
  Keep the exact-mode example. Add one sentence: on `no_match`, an
  `uncontained_match_candidates` count means the controls exist but none
  sits in a qualifying container — narrow with `scope` or revise the
  container text.
- browser-contexts.md: the existing sentence ("semantic ancestor
  relationship, not a spatial-nearness heuristic") stays true; extend it
  minimally to say bounded generic row/card wrappers qualify and
  page-scale ancestors never do.
- Regenerate `docs/public/llms-full.txt` with `bun run docs:build`; never
  hand-edit it.

**Acceptance Criteria**:
- [ ] SPEC, SKILL.md, and browser-contexts.md state the same two-tier
      eligibility rule; no doc still claims plain "nearest matching
      ancestor" semantics.
- [ ] SPEC documents `uncontained_match_candidates` reporting beside the
      relaxed-candidate reporting with the same cap/saturation language.
- [ ] `docs/public/llms-full.txt` regenerated, not hand-edited.

## Implementation Order

1. Unit 1 — core contract (cap constant, `without_container_text`,
   `uncontained_match_candidates`, constructor replacement) with its tests.
2. Unit 2 — CDP walk rewrite and uncontained scan, re-encoding the
   existing container test to the new contract.
3. Unit 3 — SPEC + plugin doc convergence and llms-full regeneration.
4. Full gate: `cargo fmt --all -- --check`,
   `bash scripts/check-wire-enum-schemas.sh`,
   `cargo check/test/clippy --workspace --all-targets --locked`.

## Simplification

- One matching rule, one contract: the walk is rewritten in place;
  `with_relaxed_candidates` is replaced (not aliased) by
  `with_no_match_diagnostics`; no compatibility shim, no dual schema.
- Reuse `RelaxedMatchCandidates` and `MAX_SEMANTIC_RELAXED_CANDIDATES` for
  the new diagnostic instead of introducing a parallel struct/constant.
- The misleading comment in the current walk ("The nearest explicit local
  container is the only authority") is replaced by comments stating the
  two-tier rule.
- The existing test intent "small generic wrapper text never qualifies"
  is deliberately retired — it encoded the defect. Its page-text
  protection is re-encoded against structural roles and the cap.
- Intentionally retained: `LOCAL_CONTAINER_ROLES` allowlist and its
  authority semantics — semantic containers still beat text heuristics.
  Layout-table roles (`LayoutTableRow` etc.) intentionally stay out of
  both registries; widen the registry later only on observed need.

## Testing

All matcher tests live in the existing `#[cfg(test)]` module of
`crates/krometrail-cdp/src/control/snapshot.rs`; contract tests live in
the existing test module of
`crates/krometrail-core/src/browser/observation.rs`.

- **Regression (the shakedown defect)** — cdp: container-qualified
  checkbox query on a fixture whose row container is `generic` with
  rendered text "Buy milk" returns `unique`. Protects the core promise of
  this feature; would have caught the v1.5.0 shakedown failure.
- **Interface (authority preserved)** — cdp: rework the existing
  `LOCAL_CONTAINER_ROLES` container test — listitem-contained checkboxes
  still resolve by their own container; a non-matching nearest listitem
  still fails even when an ancestor's text would match. Protects against
  the fallback silently weakening semantic-container authority.
- **Interface (page-scale exclusion)** — cdp: a control whose generic
  ancestor's collapsed text exceeds `MAX_GENERIC_CONTAINER_TEXT_BYTES`
  returns `no_match` (with the diagnostic), and structural/document-level
  text never qualifies. Protects the reason the allowlist existed: no
  page-root div qualifies every control.
- **Interface (transparent wrappers)** — cdp: textless generic wrapper
  between control and text-bearing row is skipped, not treated as a
  failing authority. Protects nested column markup, the most common real
  layout after flat rows.
- **Interface (diagnostic gating)** — cdp + core: `uncontained_match_candidates`
  present only on `no_match` of a container-qualified query whose stripped
  form matches; absent on success, on non-container queries, and on
  hopeless queries; coexists with `relaxed_match_candidates` on one
  result. Core side additionally covers `with_no_match_diagnostics`
  filtering and serde omission of the absent field. Protects the wire
  contract consumed by the MCP projection.
- **Unit (normalization measure)** — core: `collapsed_semantic_text_bytes`
  agrees with matcher normalization on whitespace runs and invisible
  characters. Protects cap decisions from diverging from match decisions.
- **Test removal** — the `shared_page_text` NoMatch assertion in the
  current container test is retired as written (its markup is now a
  legitimate bounded container by design) and replaced by the page-scale
  exclusion cases above.
- No new fixtures or integration harness: all cases express as the
  existing in-memory `ActiveSnapshot`/`PageSnapshot` constructions.

## Risks

- **Sibling-bleed on small repeated lists**: a `contains` container query
  can qualify a neighboring row's control when the shared wrapper sits
  under the cap, degrading `unique` to `ambiguous`. Accepted: ambiguity is
  an explicit, recoverable outcome and never authorizes action; docs steer
  to `exact` mode and `scope`. If shakedown shows this biting often, the
  cap is a single constant to tune.
- **AX tree omission of generic wrappers**: Chrome prunes some ignored
  generic nodes from the accessibility tree; if the text-bearing row div
  is absent from the snapshot, the walk consults the next generic ancestor
  (a text superset), which still matches unless over cap. The shakedown
  evidence shows the row div present with role `generic`, so the primary
  path is real; verify against the live repro during implementation, not
  just fixtures.
- **Rendered-text availability**: the fallback depends on
  `SemanticNodeMetadata.rendered_text` being populated for generic nodes
  by DOM-semantics acquisition. Existing fixtures and the current walk's
  comment indicate it is; if decode turns out to skip generic nodes, the
  fix moves to decode, not to the walk.
- **Cap miscalibration**: 1024 collapsed bytes is a judgment call (card
  with a paragraph fits; page body does not). Wrong in either direction is
  a one-constant change with SPEC wording already number-free.
- **Behavior change is intentional and uncompensated**: markup that
  previously returned `no_match` can now match (including exact-mode
  matches at an inner generic that a coarser allowlisted authority would
  have rejected). No supported consumer depends on the old silence
  (agent-tool contract, Current Contract Discipline), but reviewers should
  treat changed test expectations as the new contract, not test drift.

## Implementation notes

- Execution capability: direct-read inline implementation; the feature is one cohesive core/CDP contract and documentation change.
- Review weight: standard default; the caller explicitly keeps this feature at `stage: implementing` for the host to advance.
- Files changed: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `docs/SPEC.md`, `plugin/skills/krometrail/SKILL.md`, and `plugin/skills/krometrail/references/browser-contexts.md`.
- Tests added/re-encoded: core no-match diagnostic filtering and serde round-trip, container-query stripping, and collapsed-byte normalization tests; CDP generic-row regression, transparent-wrapper, cap/document exclusion, authority, and diagnostic-gating coverage; the prior shared-page-text no-match assertion now verifies the bounded generic-row match.
- Simplification: replaced `with_relaxed_candidates` directly with `with_no_match_diagnostics`, reused `RelaxedMatchCandidates` and its cap, and added no compatibility alias or parallel schema.
- Discrepancies from design: none in runtime behavior. `bun run docs:build` regenerated `docs/public/llms-full.txt` successfully with no diff because the generator's curated source list excludes SPEC and plugin skill files. No `CLAUDE.md` exists in the repository.
- Adjacent issues parked: none.
