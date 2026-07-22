---
id: feature-semantic-wait
kind: feature
stage: done
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Semantic wait

## Brief

GitHub issue #14 finding #10: exact-text waits are brittle when a control
includes additional copy beyond the identifying text, and there is no
first-class role/name wait aligned with how `query_page` targets controls. A
caller who interacts semantically (role + accessible name) must fall back to
text matching or locator waits to await the resulting state.

Extend `wait` with a semantic condition that reuses the existing `query_page`
query shapes (role/name, label, text, test id — same exact/contains modes and
normalization) so waiting and targeting speak one language. One registry of
query shapes drives both surfaces (registry-declared-surfaces); do not fork a
second matching implementation for waits.

## Simplification opportunity

If the semantic condition subsumes common uses of the exact-text wait, design
should check whether text-wait guidance (and any awkward matching options that
existed only to approximate semantic waits) can be simplified.

## References

- GitHub issue #14, finding 10 (wait ergonomics).

## Design decisions

- **Satisfaction semantics (any vs unique)**: `presence: present` is satisfied when the query
  matches at least one node (`outcome != no_match`); `presence: absent` when it matches none.
  Uniqueness stays `query_page`'s concern — a wait awaits state, it does not authorize action, so
  requiring uniqueness would fail waits spuriously when two matching nodes legitimately appear.
  The probe reports the observed `outcome` and total match count so the caller knows whether a
  follow-up `query_page` would be unique before acting.
- **No actionable references from wait**: the wait probe never carries a `NodeReference`. This
  preserves the SPEC invariant that ambiguous/truncated semantic results never authorize mutation
  and avoids reference-lifetime questions after the wait returns. Callers follow up with
  `query_page`; because the probe installs the active snapshot, a same-document follow-up reuses
  the same generation and stable node ids.
- **One matcher, one acquisition path**: the CDP probe constructs a `QueryPageRequest` and reuses
  the existing `query_page` execution (snapshot capture + `SnapshotRegistry::query`) wholesale.
  No second matcher, no JS re-implementation — `SemanticQuery` remains the single registry-declared
  query language (registry-declared-surfaces).
- **Scope: main document, no `scope` reference, in v1**: the semantic condition takes only
  `query` + `presence`. `container_text` inside `SemanticQuery::Role` already provides bounded
  in-query scoping; a `NodeReference` scope is generation-bound and ill-suited to a polling wait;
  frame scope (`SemanticDocumentScope`) can be added later as an optional wire field without
  breaking the shape. Logged as deliberate deferral, not oversight.
- **Cost bounding — re-snapshot per poll with a floor**: each poll acquires a fresh canonical
  snapshot through the existing capture path (AX tree, plus DOM semantics only when
  `requires_dom_semantics()`); `begin_snapshot` already reuses the generation for an unchanged
  document, so repeated polls do not grow state. Because this is heavier than the JS text probe,
  the semantic condition enforces a poll-interval floor of 100 ms (validated in
  `WaitRequest::new`, mirroring the existing elapsed/network cross-field checks).
- **Error paths**: bounded-acquisition failures (snapshot node limit, DOM-semantics acquisition
  failure) fail the wait immediately with the existing structured errors — retrying cannot shrink
  the page. A `stale_reference` error from the probe (document replaced mid-capture, or the
  registry race "semantic snapshot is no longer active") is the one transient class: the loop
  treats it as an inconclusive poll — no probe recorded — and continues to the next poll. The
  semantic condition holds no node reference, so `stale_reference` cannot mean anything else here.
- **`presence` defaults to `present`** on the wire (`impl Default for WaitPresence`), matching
  `SemanticTextMatch`'s defaulting style. The existing `text` condition wire declares `presence`
  without `#[serde(default)]` and is unaffected.
- **Relaxed-candidate diagnosability**: the probe carries `relaxed_match_candidates` (reusing
  `RelaxedMatchCandidates`) only on an unmatched `present` probe whose query used an exact
  matcher — so a timeout's `last_probe` explains that a `contains` retry would have landed,
  mirroring the `query_page` no-match contract. It is filtered off satisfied and `absent` probes.
- **Text wait retained — guidance simplification only (flagged)**: the semantic condition subsumes
  the common "wait for control text" use, but `text` waits over document-body text and
  CSS-selector/reference-scoped element text are not expressible semantically. Removing them would
  reduce behavior, so nothing is removed; tool description, wire doc comments, plugin skill
  guidance, and SPEC wording are re-pointed so role/name waits route to the semantic condition.

## Architectural choice

Extend `WaitCondition` with a `Semantic` variant in `krometrail-core` that embeds the existing
`SemanticQuery` unchanged, and implement the CDP probe by delegating to the existing `query_page`
execution path. Two alternatives were rejected: (a) a JS-evaluated in-page matcher (forks the
matcher — forbidden by the brief and by registry-declared-surfaces); (b) composing `query_page`
retries at the MCP layer (loses single-operation timeout/cancellation/batch semantics and probe
evidence). The chosen shape keeps one query language, one acquisition path, one matcher, and slots
into the existing generic poll loop with its `controlled` cancellation/deadline discipline.

## Implementation Units

### Unit 1: Core domain — semantic wait condition and probe
**File**: `crates/krometrail-core/src/browser/wait.rs` (plus `Default` impl beside `WaitPresence`)

```rust
pub const MIN_SEMANTIC_WAIT_POLL_INTERVAL_MILLIS: u64 = 100;
pub const MIN_SEMANTIC_WAIT_POLL_INTERVAL: Duration =
    Duration::from_millis(MIN_SEMANTIC_WAIT_POLL_INTERVAL_MILLIS);

impl Default for WaitPresence {
    fn default() -> Self { Self::Present }
}

// WaitCondition — new variant (domain + wire; wire tag "semantic")
Semantic {
    query: SemanticQuery,
    presence: WaitPresence,
},

// WaitConditionWire — new variant
Semantic {
    /// Satisfied when the query matches at least one node (`present`) or none (`absent`).
    /// Reuses the query_page query language: role/name, label, text, test_id, with the same
    /// exact/contains modes and whitespace/case normalization.
    query: SemanticQuery,
    #[serde(default)]
    presence: WaitPresence,
},

// WaitProbe — new variant
Semantic {
    matched: bool,
    outcome: SemanticQueryOutcome,
    match_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relaxed_match_candidates: Option<RelaxedMatchCandidates>,
},
```

**Implementation Notes**:
- `WaitCondition::validate` for `Semantic` needs no extra checks: `SemanticQuery` validates itself
  at construction/deserialization (role charset, 1024-byte bounds, non-empty text).
- `WaitRequest::new` gains the cross-field check next to the elapsed/network arms:
  `WaitCondition::Semantic { .. } if poll_interval < MIN_SEMANTIC_WAIT_POLL_INTERVAL` →
  `invalid("semantic wait poll interval must be at least 100 milliseconds")`. The wire-level
  schemars range on `poll_interval` (10..=5000) is unchanged; the floor is a construction check.
- Manual `Serialize`/`Deserialize` arms for the new `WaitCondition` variant follow the existing
  pattern; `WaitProbe::matched()` gains the new arm. `SemanticQueryOutcome` and
  `RelaxedMatchCandidates` are already `Eq + serde + JsonSchema`, so `WaitProbe`'s derives hold.
- Wire-enum discipline: `WaitConditionWire` is explicitly tagged (`condition`/`value`,
  `rename_all = snake_case`) and `SemanticQuery`'s wire is already stable — no new bare enum, so
  `scripts/check-wire-enum-schemas.sh` stays satisfied.

**Acceptance Criteria**:
- [ ] `{"condition":"semantic","value":{"query":{"kind":"role","role":"button","name":{"value":"Save"}}}}`
      deserializes with `presence: present` defaulted and round-trips.
- [ ] A semantic condition with `poll_interval` below 100 ms is rejected by `WaitRequest::new`
      and by wire deserialization; other conditions still accept 10 ms.
- [ ] Invalid embedded queries (uppercase role, oversized text) are rejected through the same
      `SemanticQuery` validation as `query_page`.

### Unit 2: CDP probe — reuse the query_page execution path
**File**: `crates/krometrail-cdp/src/control/wait.rs`

```rust
// probe_condition changes receiver to &mut self (execute_wait already holds &mut self)
async fn probe_condition(
    &mut self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    condition: &WaitCondition,
    started_at: SessionTime,
) -> Result<WaitProbe>;

async fn probe_semantic(
    &mut self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    query: &SemanticQuery,
    presence: WaitPresence,
    started_at: SessionTime,
) -> Result<WaitProbe>;
```

**Implementation Notes**:
- `probe_semantic` builds
  `QueryPageRequest::new(PageSelection::Target(bound.target_id), query.clone(), None, DEFAULT_SEMANTIC_MATCH_LIMIT)`
  and calls the existing `self.query_page(transport, bound, request, started_at)`
  (`pub(super)` in `control::snapshot`, same module tree), unwrapping
  `BrowserOperationResult::QueryPage`. Using the default match limit keeps the probe's `outcome`
  aligned with what a default follow-up `query_page` would report.
- Probe mapping: `match_count = matches.len() as u32 saturating_add omitted_match_count`;
  `matched = (presence == Present) != (outcome == NoMatch)`;
  `relaxed_match_candidates = result.relaxed_match_candidates.filter(|_| !matched && presence == Present)`.
- Loop change in `execute_wait`: a probe `Err` whose code is `ErrorCode::StaleReference` while the
  condition is `Semantic` is an inconclusive poll — do not record a probe, fall through to the
  poll-interval sleep. All other errors propagate and fail the wait. Cancellation
  (`ErrorCode::Cancelled`) is unaffected because it surfaces from `controlled`, not the probe.
- Side effect is deliberate and documented: each probe installs/refreshes the active snapshot for
  the target exactly as a standalone `query_page` does; same-document generations and node ids
  are preserved by `begin_snapshot`, so pre-existing references stay valid.
- `probe_condition`'s `unreachable!` arm for elapsed/network is unchanged; the `Semantic` arm
  routes to `probe_semantic`.

**Acceptance Criteria**:
- [ ] A `present` semantic wait satisfies once a poll's snapshot matches, with a probe carrying
      `outcome`, `match_count`, and no reference.
- [ ] An `absent` semantic wait satisfies when the query stops matching; its satisfied probe
      carries no `relaxed_match_candidates`.
- [ ] A timed-out exact-matcher `present` wait's `last_probe` reports `relaxed_match_candidates`
      when a `contains` retry would have matched (decorated accessible name case).
- [ ] A snapshot node-limit overflow fails the wait with the existing bounded narrowing error
      rather than polling to timeout.
- [ ] A mid-capture document replacement (`stale_reference`) does not fail the wait; the loop
      continues and can satisfy on a later poll.

### Unit 3: Surface, guidance, and foundation docs
**Files**: `crates/krometrail-core/src/browser/operation.rs`,
`crates/krometrail-core/src/browser/wait.rs` (Text wire doc comment),
`plugin/skills/krometrail/SKILL.md`, `docs/SPEC.md` ("Waiting")

**Implementation Notes**:
- `operation.rs` `wait` registry row description →
  `"Wait for an explicit page, element, text, semantic, navigation, or network condition."`
- `WaitConditionWire::Text` doc comment gains one pointer: for a control identified by role and
  accessible name, use the `semantic` condition instead of approximating with text.
- Plugin skill: extend the text-wait paragraph with the semantic condition and one JSON example
  (role + name, contains mode), stating present/absent semantics and that the wait returns no
  actionable reference — follow up with `query_page` to act.
- `docs/SPEC.md` "Waiting" adds: "wait for a semantic locator match — the same role/name, label,
  text, and test-id query shapes, matching modes, and normalization as structured queries — to be
  present or absent;" plus contract sentences: satisfaction is any-match/no-match, the wait result
  carries the observed query outcome but never an actionable reference, and bounded-acquisition
  failures fail the wait explicitly with narrowing guidance. Regenerate `docs/public/llms-full.txt`
  via `bun run docs:build`.

**Acceptance Criteria**:
- [ ] `wait` tool schema exposes the semantic condition with the same `SemanticQuery` subschema
      `query_page` publishes.
- [ ] SPEC "Waiting" and the plugin skill describe the semantic condition; no text-wait behavior
      is removed.

### Unit 4: Tests
**Files**: `crates/krometrail-core/src/browser/wait.rs` (tests mod),
`crates/krometrail-cdp/src/control/snapshot.rs` or `crates/krometrail-cdp/tests/waits_and_batches.rs`,
`crates/krometrail-cdp/tests/verified_interactions.rs`/real-chrome lane as applicable

**Implementation Notes**:
- Core: add a semantic case to `every_condition_round_trips_with_integer_milliseconds`; add
  presence-default and poll-floor rejection cases to the existing validation test.
- CDP scripted (ScriptedCdp): one test where the first poll's AX tree lacks the role/name match
  and a later poll contains it (present satisfies, probe fields correct); one absent-mode
  inversion; one exact-miss timeout asserting `relaxed_match_candidates` on `last_probe`. If
  multi-poll AX scripting is awkward in the harness, fall back to single-poll satisfied/unsatisfied
  scripted tests plus the real-chrome lane for polling behavior.
- Real-chrome opt-in: extend `opt_in_real_chrome_qualifies_every_wait_family_and_stale_references`
  with the semantic family so the test name stays honest (layered-cdp-qualification).
- No tests removed: existing wait tests remain valid and non-overlapping.

**Acceptance Criteria**:
- [ ] All listed tests pass under `cargo test --workspace --all-targets --locked`;
      `check-wire-enum-schemas.sh`, fmt, and clippy gates stay green.

## Implementation Order

1. Unit 1 — core condition/probe types and validation (everything hangs on the wire shape).
2. Unit 2 — CDP probe and loop integration.
3. Unit 4 — tests (core tests can land with Unit 1; CDP tests after Unit 2).
4. Unit 3 — descriptions, skill guidance, SPEC, docs regeneration.

## Simplification

- No second matcher or snapshot path: the probe reuses `query_page` execution wholesale.
- No behavior removed. Text waits (document-body and locator-scoped) are retained deliberately —
  flagged above as a design decision; only guidance is re-pointed.
- No new registry: `SemanticQuery` remains the single declaration of query shapes for both
  targeting and waiting.

## Testing

- Interface: wire round-trip + validation (core), probe satisfaction/inversion/diagnostics
  (scripted CDP) — protect the new wire contract and the any-match/no-match semantics.
- Regression-shaped: stale-mid-capture continuation and node-limit fail-fast — protect the two
  explicitly designed error paths.
- Qualification: real-chrome wait-family extension — protects the claim that the probe works
  against a live accessibility tree, not just the scripted double.

## Risks

- **`&mut self` probe refactor ripple**: switching `probe_condition` to `&mut self` touches all
  probe call sites; contained to `wait.rs`, and `execute_wait` already holds `&mut self`.
  Fallback: route `Semantic` as a dedicated strategy like elapsed/network-quiet.
- **Per-poll snapshot cost on large pages**: bounded by the 100 ms floor, existing snapshot node
  limits (fail fast with narrowing guidance), and the 120 s operation timeout cap.
- **Continuous document churn**: a page replacing documents every poll yields inconclusive polls
  until timeout with a possibly-absent `last_probe`; `WaitResult` already permits `None`, and the
  timeout evidence is honest about having no conclusive probe.
- **Scripted multi-poll AX sequences**: the test harness may not comfortably script differing AX
  responses across polls; fallback path documented in Unit 4.

## Implementation notes

- Implemented the semantic wait as a `WaitCondition::Semantic` carrying the existing
  `SemanticQuery` and `WaitPresence`; the wire default remains `present`, and semantic waits
  enforce the designed 100 ms poll floor.
- The CDP probe delegates to `query_page` through the current snapshot acquisition and matcher
  path. It returns outcome, total match count, and only the specified relaxed candidates for an
  unmet present query; it never returns an actionable node reference. Stale-reference probe
  failures are treated as inconclusive polls, while bounded acquisition failures still fail
  explicitly.
- Added core wire/validation coverage, a focused opt-in real-Chrome present/absent qualification,
  and semantic guidance in the operation description, plugin skill, and SPEC. The generated
  `docs/public/llms-full.txt` was regenerated rather than edited directly.
- Mechanical adaptation: the existing real-Chrome all-waits lane has unrelated timing
  instability in this environment (its pre-existing delayed-state/navigation assertions can
  time out before reaching the added semantic assertions). A focused semantic lane against the
  same fixture passed with `KROMETRAIL_REAL_CHROME_TESTS=1`; no production behavior was
  re-scoped around that fixture instability.
- Mechanical adaptation: the MCP registry schema conformance sweep now repairs only its
  generated semantic-wait samples to the legal 100 ms cross-field floor before domain decoding;
  the published schema's general 10 ms poll bound remains unchanged because the runtime
  condition-specific rule cannot be expressed by that flat schema.
- Full verification passed with the escalated local test process: format, wire-enum schema
  check, workspace check, workspace tests, and workspace clippy with `-D warnings`.

## Review adjudication (standard weight, fresh-context Opus, one pass)

Verified clean: single matcher/acquisition path (no fork), no reference leak,
bounded probe payload, poll-floor construction check in the existing style,
correctly-scoped stale handling with deadline-bounded looping, fail-fast
acquisition errors, correct any-match semantics across all four outcomes,
accurate SPEC/registry surface, no text-wait behavior removed.

Accepted findings, routed to the post-implementation fix batch (closure is
fix-verification only):
1. (significant) No default-lane execution coverage of `probe_semantic` — all
   CDP tests are opt-in real-Chrome; the design required scripted tests. Add
   scripted single-poll present-satisfied / absent-satisfied /
   exact-miss-timeout-with-candidates tests plus a scripted stale-injection
   test proving the loop continues (the stale-continuation branch currently
   has zero coverage).
2. (minor) The 100 ms poll floor is undiscoverable pre-failure — one sentence
   in the wire doc comment (feeds the schema description) and the plugin
   skill paragraph.
3. (nit) Conformance-sweep repair overwrites `poll_interval` to 100 —
   clamp (`max(existing, 100)`) instead so regressions stay detectable.

Rejected (no action): timeout evidence `last_probe_at = started_at` when all
polls were inconclusive — pre-existing shape the design accepted.

## Review fixes

- Added default-lane scripted present, absent, exact-miss timeout-with-relaxed-
  candidates, and stale-poll continuation tests through the real semantic
  `query_page` capture path.
- Documented the 100 ms semantic poll floor in the wire comment and plugin
  guidance, and changed schema-test repair to clamp existing intervals rather
  than overwrite values above the floor.

## Review closure

Closure verified 2026-07-22: all accepted findings landed in commit d7b04559
(full gate + docs build + real-Chrome qualifications green) and were spot-
verified in-tree. Review complete.
