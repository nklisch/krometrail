---
id: feature-exact-query-native-control-miss
kind: feature
stage: done
tags: [agent-ux, browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Exact semantic queries can miss visible native controls

## Brief

GitHub issue #14 finding #4: a visible, enabled native button with rendered
text, `aria-expanded`, and `aria-controls` was absent from exact role/name and
exact-text `query_page` results, though the query reported a relaxed candidate.
CSS-selector activation of the same control worked and toggled `aria-expanded`,
so the control was real, actionable, and semantically labeled.

- Coordinate interaction: `9a443e44-e362-43ad-9f3c-6a2066cc0ba9`.
- CSS-selector interaction: `dc1c03c2-20fc-4e88-a9ef-a885e1f90dac`.

The relaxed-candidate surface behaved as designed; the defect is that exact
matching skipped a control it should have matched. Root-cause the accessible-
name computation / normalization path used by exact role/name and exact-text
matching for native controls (whitespace or nested-content normalization,
name-from-content rules, or state-bearing attributes affecting the computed
name are likely suspects), then fix exact matching to find it. Reproduce with a
deterministic fixture control mirroring the reported shape before fixing.

## Simplification opportunity

None identified; this is a bounded matching-fidelity fix inside the existing
query contract.

## References

- GitHub issue #14, finding 4 (macOS, authenticated local React app).

## Root-cause analysis

The exact-matching chain, fully traced:

1. `query_page` acquires the AX tree and stores each node's accessible name
   **verbatim** from Chrome (`ax_owned`,
   `crates/krometrail-cdp/src/control/snapshot.rs:1890,2020`). Rendered text for
   text queries is the verbatim join of `DOMSnapshot.captureSnapshot` layout text
   runs, per-run whitespace-collapsed and space-joined, propagated to every
   ancestor (`snapshot.rs:1021-1050`, `append_semantic_text` at
   `snapshot.rs:1438-1460`).
2. Role/name matching compares `node.role` and runs the name matcher over that
   verbatim name (`semantic_query_matches`, `snapshot.rs:772-777`); text matching
   runs the matcher over the verbatim rendered text (`snapshot.rs:783`).
3. `SemanticTextMatch::matches` normalizes both sides with
   `normalize_semantic_text`
   (`crates/krometrail-core/src/browser/observation.rs:606-613, 628-647`), which
   trims, collapses runs of Unicode `White_Space`, and optionally lowercases —
   **and nothing else** — then requires `candidate == expected` for exact mode.

**Primary hypothesized cause**: the computed accessible name / rendered text of a
native disclosure button routinely carries *invisible or iconic non-whitespace
codepoints* adjacent to the visible words, which survive normalization and defeat
`==` while `contains` still hits — exactly the reported signature (exact miss on
both role/name and text, relaxed candidate present, CSS activation fine). Concrete
producers, all standard for a chevroned `aria-expanded` button in a React app:

- icon-font glyph text (Private Use Area codepoints, e.g. `\u{E5CF}` from
  Material Icons ligature spans) inside the button — enters both Chrome's
  name-from-contents and the layout text runs;
- CSS `::before`/`::after` generated content (icon-font chevrons) — accname
  includes generated content, and `DOMSnapshot` emits pseudo-element text runs
  that `append_semantic_text` folds into the ancestor button's rendered text;
- zero-width format characters (`\u{200B}` ZWSP, `\u{200D}` ZWJ, `\u{FEFF}`,
  soft hyphen `\u{00AD}`) from JSX/text pipelines — `char::is_whitespace` is the
  Unicode `White_Space` property, which excludes all of these
  (`observation.rs:631-634`).

Secondary hypotheses the fixture must also probe: (a) visually-hidden (sr-only,
clip-pattern) suffix text inside the button — participates in layout and accname,
so it decorates the name; if that is the observed shape, exact-miss-plus-relaxed-
candidate is the *designed* decorated-name behavior, not a defect; (b) an
`aria-label` differing from the rendered text — also by design. The original app
is not available to this repository, so reproduction ordering below decides.

`aria-expanded`/`aria-controls` themselves are inert here: they land in
`properties` via the allowlist (`snapshot.rs:20-37`) and never touch name
computation or matching.

## Design decisions

- **Fix location is the shared normalizer, symmetrically**: extend
  `normalize_semantic_text` so both query and candidate drop what renders no
  interoperable text — remove invisible format characters (explicit Cf list:
  soft hyphen, `\u{200B}`-`\u{200F}`, `\u{2060}`-`\u{2064}`, `\u{202A}`-`\u{202E}`,
  `\u{061C}`, `\u{FEFF}`) with no separator (they are zero-width, possibly
  mid-word), and treat Private Use Area codepoints (`\u{E000}`-`\u{F8FF}`, planes
  15-16) as whitespace-equivalent separators (they render as standalone icon
  glyphs). Explicit character lists over a Unicode-tables dependency — the
  practical set is small and a new dependency is not justified.
- **Reproduce before fixing, but the matcher defect stands on its own**: the
  qualification fixture decides whether real Chrome produces such names for the
  reported shape; independently, the deterministic red test proves the matcher
  misses on names containing these codepoints, which is a genuine
  matching-fidelity defect inside the existing query contract regardless of the
  original app's exact markup. If qualification shows Chrome never emits the
  hypothesized shapes AND the sr-only/aria-label variants explain the report, the
  finding is decorated-name behavior working as designed — record the captured AX
  names as evidence in this item and re-scope before landing behavior change.
- **Relaxed-candidate surface untouched**: it behaved as designed and remains the
  guidance path for genuinely decorated names (sr-only suffixes, badge counts,
  `"Cargo.toml, (File)"`).
- **Icon-only controls**: after stripping, a name that is purely a PUA glyph
  normalizes to empty and can no longer text-match; such controls remain
  reachable by role, container text, test id, or CSS selector. Accepted — an
  icon glyph codepoint was never a usable agent query key.
- **No child stories**: single bounded fix with one cohesive acceptance surface;
  checkpoints would be pure overhead per the checkpoint rule.

## Architectural choice

Considered:

1. **Harden `normalize_semantic_text`** (chosen) — one shared authority already
   used symmetrically by exact and contains modes for role/name, label, text, and
   container-text matching; smallest change; fixes selector-free targeting for
   every query kind at once.
2. Strip at acquisition time (sanitize names/rendered text during snapshot
   decode). Rejected: presentation and drill-down (`snapshot_page`) should keep
   reporting the real accessible name — canonical-result-projection says derive
   matching behavior without rewriting canonical evidence.
3. Add a new "visible-text" match mode. Rejected: expands the wire surface for
   what is a fidelity bug in the existing exact contract; agents should not need
   to know a second mode exists.

## Implementation Units

### Unit 1: Deterministic fixture control mirroring the reported shape
**File**: `tests/fixtures/browser/verified-interactions/index.html`

A disclosure group: visible, enabled native
`<button id="disclosure-toggle" aria-expanded="false" aria-controls="disclosure-panel">`
with rendered text plus decoration variants, a `#disclosure-panel` region, and a
click handler toggling `aria-expanded`/panel visibility. Variants (distinct
visible texts so each is independently queryable):

- inline icon-font span with a PUA codepoint after the text;
- CSS `::after { content: "\2795" or PUA glyph }` chevron;
- zero-width space embedded in the text;
- sr-only (clip-pattern) suffix span (secondary-hypothesis probe);
- undecorated control (positive control — must match before and after).

**Acceptance Criteria**:
- [ ] Fixture control is a native button, visible and enabled, with rendered
      text, `aria-expanded`, and `aria-controls`; clicking toggles
      `aria-expanded` (mirrors correlations `9a443e44…`/`dc1c03c2…`).

### Unit 2: Red tests proving the miss
**Files**: `crates/krometrail-cdp/src/control/snapshot.rs` (test module, modeled
on `exact_no_match_reports_how_many_nodes_a_contains_retry_would_reach` at
`snapshot.rs:3365`), `crates/krometrail-core/src/browser/observation.rs` (test
module), `crates/krometrail-cdp/tests/verified_interactions.rs`

```rust
// snapshot.rs tests — Chrome-shaped payload, pre-fix red:
fn exact_role_name_and_text_match_names_carrying_invisible_codepoints()
// name: "Advanced filters\u{200B}", "Filters \u{e5cf}" — exact query "Advanced filters"/"Filters"
// pre-fix: no match + relaxed_match_candidates == 1; post-fix: unique match

// observation.rs tests — matcher level:
fn exact_match_ignores_format_characters_and_private_use_glyphs()

// verified_interactions.rs — real-browser qualification:
// exact role/name and exact text query for each fixture variant must return one
// unique reference; on failure the test output records the full-snapshot AX name
// bytes for the missed control (the reproduction evidence this item requires).
```

**Acceptance Criteria**:
- [ ] Deterministic tests are red against the current normalizer for the
      PUA/format-character variants and green after Unit 3.
- [ ] Qualification either reproduces the miss on at least one variant (evidence:
      captured AX name) or this item's body records the captured names proving
      real Chrome does not emit the hypothesized shapes for any variant.

### Unit 3: The fix
**File**: `crates/krometrail-core/src/browser/observation.rs` (628-647)

```rust
fn normalize_semantic_text(value: &str, case_sensitive: bool) -> String
// unchanged signature; loop gains:
//   if is_invisible_format(character) { continue; }            // removed, no separator
//   if is_private_use(character) { pending_space = !normalized.is_empty(); continue; }
const fn is_invisible_format(character: char) -> bool
const fn is_private_use(character: char) -> bool
```

**Implementation Notes**:
- Applies to every matcher path (role/name, label, text, container text) and to
  both exact and contains modes, query side and candidate side — symmetry keeps
  contains-mode behavior consistent with exact mode.
- `docs/SPEC.md` "Structured Page Snapshots": one sentence — exact and contains
  text matchers compare whitespace-collapsed text ignoring invisible format
  characters and private-use icon glyphs. Regenerate `docs/public/llms-full.txt`.

**Acceptance Criteria**:
- [ ] All Unit 2 tests green; existing snapshot/query/relaxed-candidate tests
      unchanged and green (`cargo test --workspace --all-targets --locked`).
- [ ] Undecorated fixture control still matches (no regression in the positive
      control).

## Implementation Order
1. Unit 1 (fixture) + Unit 2 qualification probe — reproduction evidence first.
2. Unit 2 deterministic red tests encoding the confirmed (or hypothesized,
   see decision) shapes.
3. Unit 3 fix + SPEC sentence; re-run the ladder.

## Simplification
- No new wire surface, match mode, or error path; one normalizer function grows
  two const character classifiers. Nothing identified for removal — the relaxed
  candidate mechanism stays as the complementary guidance surface.

## Testing
- Matcher-level unit tests (genuinely complex isolated logic: Unicode class
  edges — mid-word ZWSP, leading/trailing PUA, PUA-only names normalizing to
  empty and matching nothing).
- Interface tests at the query seam (scripted snapshot payloads) protecting the
  exact-match contract and the relaxed-candidate interplay.
- Real-browser qualification protecting against Chrome accname/DOMSnapshot drift
  and providing the root-cause evidence trail.
- No test removals identified.

## Risks
- **The original miss may not be codepoint-driven** (no access to the reporting
  app). Mitigated by reproduction-first ordering, five fixture variants, and the
  captured-AX-name evidence requirement; if a different cause surfaces (e.g., a
  semantic-join gap), record it here and re-design before fixing.
- **Over-stripping**: removing soft hyphens/bidi marks could conflate two names
  that authors meant to distinguish — implausible for agent-facing controls, and
  ambiguity still surfaces as the existing multiple-match outcome rather than a
  wrong single action (semantic matching never silently selects one of several).

## Implementation notes

- Added the five-variant native disclosure fixture: inline PUA content, CSS pseudo-element PUA
  content, an embedded zero-width space, a clipped screen-reader-only suffix, and an undecorated
  positive control. Every button is visible/enabled, carries `aria-expanded`/`aria-controls`, and
  toggles its panel when clicked.
- Reproduction verdict: the codepoint hypothesis reproduced in real Chrome. Before the fix,
  exact role/name and exact rendered-text queries missed the inline PUA, pseudo-PUA, and ZWSP
  controls. The captured AX names were `Inline PUA disclosure \u{e5cf}` and
  `Pseudo PUA disclosure \u{e5cf}` (UTF-8 suffix bytes `[238, 151, 143]`), and
  `Zero\u{200b}width disclosure` (UTF-8 bytes `[226, 128, 139]`). The screen-reader-only
  variant produced the intentionally decorated name `Screen reader suffix disclosure extra
  details`; its exact visible-text query remained a designed no-match with a relaxed candidate.
  The undecorated control matched. Because Chrome did emit the hypothesized codepoints, no
  re-scope away from the matcher fix was warranted.
- Added the matcher-level red test and scripted snapshot seam red test before implementation;
  both became green after extending the shared normalizer. The fix strips the explicit invisible
  format list without inserting separators, treats BMP and supplementary private-use codepoints
  as separator whitespace, and rejects an all-invisible/all-icon normalized query as a text key.
  This applies symmetrically to exact/contains role-name, label, rendered-text, and
  container-text matching.
- The gated real-Chrome test now qualifies the three codepoint variants and positive control, and
  records the full-snapshot AX name bytes for the expected decorated sr-only boundary. The
  disclosure click also verified `aria-expanded` toggling.
- Regenerated `docs/public/llms-full.txt` through `bun run docs:build`; no generated file was
  edited directly. Full verification passed with the escalated local test process: format,
  wire-enum schema check, workspace check, workspace tests, and workspace clippy with
  `-D warnings`.

## Review adjudication (standard weight, fresh-context Opus, one pass)

Verified clean: single shared normalizer authority (symmetric, all query
kinds), exact codepoint lists per design, icon-only guard, genuinely-red
tests, AX-byte evidence on real-Chrome failure, canonical evidence untouched,
relaxed-candidate surface unchanged. Real-Chrome reproduction confirmed the
codepoint hypothesis.

Findings: (minor, accepted) the sr-only intentional-boundary assertion runs
only in a conditional branch — a future over-stripping regression would pass
silently; hoist to an unconditional NoMatch pin. (nit, rejected) ZWJ stripping
can conflate emoji-sequence names — design-accepted risk, surfaces as
Ambiguous, never a wrong single action. Fix routed to the post-implementation
batch; closure is fix-verification only.

## Review fixes

- Hoisted the screen-reader-only disclosure variant out of the shared
  qualification loop and now unconditionally pins `NoMatch` for both exact
  role/name and exact-text forms, preserving the intentional decorated-name
  boundary.

## Review closure

Closure verified 2026-07-22: all accepted findings landed in commit d7b04559
(full gate + docs build + real-Chrome qualifications green) and were spot-
verified in-tree. Review complete.
