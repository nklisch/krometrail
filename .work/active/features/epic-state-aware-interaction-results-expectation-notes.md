---
id: epic-state-aware-interaction-results-expectation-notes
kind: feature
stage: done
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results
depends_on: [epic-state-aware-interaction-results-postcondition-core, epic-state-aware-interaction-results-side-channel-outcomes]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Expectation notes

## Brief

The interpretive layer, deliberately last and deliberately small: when a
common expectation for the dispatched action observably did not hold, the
interaction result carries at most one conservative expectation note — for
example, a link activation with no navigation, no new page, and no download; or
a checkbox click with no checked-state delta. The note is descriptive ("the
click dispatched and no navigation or page change was observed"), never a
failure claim, per the epic's locked strategic decision. This addresses the
issue #14 finding #1 ask that the result warn when the semantic postcondition
differs from likely intent.

Expectations are declared in the existing browser-operation registry (the
`ActionDefinition` table that already declares category, actionability, and
completion per operation) keyed by action kind and target role — one registry
declaration, not a parallel expectation table. Note derivation is a pure
function over the postcondition facts produced by `postcondition-core` and
`side-channel-outcomes`; it introduces no new observation work.

Does NOT cover: any new fact capture (upstream features own facts), and any
verdict language — the note never says "failed", "broken", or "bug".

## Advisory constraints (binding, from the epic's cross-model adjudication)

Negative notes require a completeness gate: each channel an expectation
depends on (navigation signal, node state, page cursor, download cursor)
carries a typed observation state — changed / unchanged / unavailable /
not-applicable, with what it was observed through — and a "did not hold" note
is emitted only when every required channel was successfully observed.
Anything less becomes "expectation not evaluated", never "no effect
observed". Role-based expectations are suppressed when the target role is
unavailable (coordinate actions, unresolved selectors).

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: consumer of both fact-producing features; the epic's
  highest false-signal-risk surface, so it lands after the facts are proven.

## Simplification opportunity

- Expectations extend the existing operation registry; do not introduce a
  second registry or per-tool special cases.
- Async/deferred applications legitimately defer effects past the observation
  point — the conservative bar (observed facts only, one note, descriptive
  wording, observation-point framing) is the mitigation; design should not add
  configurable sensitivity knobs in v1.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (at most one conservative
  expectation note)
- `docs/ARCHITECTURE.md` — Capability Registry, Interaction Execution
- GitHub issue #14, finding #1

## Design decisions

- **V1 expectation vocabulary**: declare five small rules in the existing
  `ActionDefinition` registry: link `click` expects at least one of committed
  navigation, new page, or download; checkbox/radio `click` expects a checked
  delta; any other role-known `click` with an observed `expanded` fact expects
  an expanded delta; `fill` expects a value-length delta; and `select_option`
  expects a selected delta. `hover`, `drag`, `scroll`, `upload_files`,
  `press_keys`, and `handle_dialog` have no v1 negative expectation. The
  `select_option` rule is deliberately completeness-gated even though the
  landed native `<select>` probe normally reports `selected: None`; it therefore
  produces `not evaluated`, not a misleading note, until an upstream fact can
  establish selected-option identity. This feature adds no observation work.
- **Role authority**: classify the accessibility role retained by the active
  snapshot node used for a reference locator into `link`, `checkbox`, `radio`,
  or `other`. CSS selectors and coordinate/target-wide actions carry no role;
  they never receive a role-based negative note. Reusing the resolved snapshot
  binding is the minimal bounded path and avoids a second accessibility query
  or a guessed role from HTML tags.
- **Typed completeness gate**: normalize every candidate channel to
  `Changed { observed_through }`, `Unchanged { observed_through }`,
  `Unavailable`, or `NotApplicable`. A positive observation on any alternative
  channel satisfies the expectation even if another channel is unavailable. A
  negative note requires every channel named by the selected expectation to be
  `Unchanged`; no changed channel plus any unavailable channel is `NotEvaluated`
  and emits no note. `None`, `TargetNodeOutcome::{NotEvaluated, Unobserved,
  DetachedOrReplaced}`, absent page/download postconditions, and a missing
  target role all map to unavailable/not-evaluated, never unchanged.
- **Navigation channel**: `main_frame_navigation_observed: Some(true)` or
  `url_changed: Some(true)` proves change; `main_frame_navigation_observed:
  Some(false)` proves the channel was observed unchanged; a missing committed
  navigation signal with no positive URL delta is unavailable. The broader
  boolean `navigation_lifecycle_observed` is not sufficient for a negative
  claim and remains supporting completion evidence only.
- **Priority and one-note cap**: registry declaration order selects the first
  role-matching expectation and does not fall through when that expectation is
  unavailable: link navigation/new-page/download, then checkbox/radio checked,
  then generic expanded, followed by the single rules for fill and select.
  This prevents an unavailable higher-intent rule from producing a lower-value
  warning opportunistically and structurally enforces at most one note.
- **Persisted placement**: persist the role classification and typed note on
  `InteractionRecord`, then project that same note into concise results. This
  keeps retained timeline reads and the live MCP response on one canonical
  interpretation and prevents a later registry edit from changing what an old
  interaction reported. Projection-only derivation would still require a
  persisted role and would make non-MCP record consumers reimplement policy.
  Because popup/download facts attach only at the session seam after the CDP
  control path returns, `execute_operation` finalizes the note after both
  enrichments and before `persist_result_evidence`. Adding required record
  fields changes opaque `record_json`, so the current store schema advances
  from v10 to v11 in the same stride; v10 is cleared as incompatible recording
  cache, with no migration or dual reader.
- **Exact wording**: persist a closed note kind and render one stable,
  observation-framed sentence: `No navigation, new page, or download was
  observed by the observation point.`; `The target's checked state was
  unchanged by the observation point.`; `The target's expanded state was
  unchanged by the observation point.`; `The target's value length was
  unchanged by the observation point.`; or `The target's selected state was
  unchanged by the observation point.` No template uses `failed`, `broken`,
  `blocked`, `no-op`, or a causal verdict.
- **Execution shape**: direct-read design and one cohesive implementation
  owner. The evaluator, role handoff, finalization seam, projection, and schema
  bump are tightly coupled and small enough that child stories would add
  bookkeeping without a useful checkpoint.

## Architectural choice

Three placements were considered. Deriving at the MCP projector would keep the
record smaller, but role would still need to survive persistence and registry
changes could reinterpret retained actions. Deriving in the CDP control path
would keep policy near target resolution, but it runs before page/download
reconciliation and cannot satisfy the completeness gate. The chosen approach is
a pure core evaluator whose result is finalized on `InteractionRecord` at the
session enrichment seam, after all already-observed facts are attached and
before the record is persisted or projected. This preserves the parent epic's
one-record authority while adding no browser command, wait, probe, or parallel
expectation table.

## Implementation Units

### Unit 1: Registry-declared expectation vocabulary and pure completeness evaluator

**Files**: `crates/krometrail-core/src/browser/postcondition.rs`,
`crates/krometrail-core/src/browser/interaction.rs`,
`crates/krometrail-core/src/browser/operation.rs`,
`crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/lib.rs`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationTargetRole { Link, Checkbox, Radio, Other }

impl ExpectationTargetRole {
    pub fn from_accessibility_role(role: &str) -> Self;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectationTarget {
    Role(ExpectationTargetRole),
    AnyObservedRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectationChannel {
    Navigation,
    NewPage,
    Download,
    Checked,
    Expanded,
    ValueLength,
    Selected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionExpectation {
    pub target: ExpectationTarget,
    pub required_channels: &'static [ExpectationChannel],
    pub note: ExpectationNote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectationNote {
    NavigationOutcomeUnobserved,
    CheckedStateUnchanged,
    ExpandedStateUnchanged,
    ValueLengthUnchanged,
    SelectedStateUnchanged,
}

impl ExpectationNote {
    pub const fn message(self) -> &'static str;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectationEvaluation {
    Held,
    DidNotHold(ExpectationNote),
    NotEvaluated,
    NotApplicable,
}

pub(crate) fn evaluate_expectations(
    expectations: &[InteractionExpectation],
    target_role: Option<ExpectationTargetRole>,
    facts: &InteractionPostcondition,
) -> ExpectationEvaluation;

pub struct ActionDefinition {
    // existing fields
    pub expectations: &'static [InteractionExpectation],
}
```

**Implementation Notes**:

- Add private `ObservationSource` and `ChannelObservation` types in
  `postcondition.rs`; sources are `MainFrameNavigationSignal`, `UrlDelta`,
  `TargetStateProbe`, `PageCursorReconciliation`, and `DownloadCursorAuthority`.
  Non-required channels normalize to `NotApplicable` and are excluded from the
  selected rule's gate.
- A new-page/download channel is changed when its bounded list is non-empty or
  `omitted > 0`, unchanged only when the corresponding `Option` is present and
  both list and omission count are zero, and unavailable when the parent is
  `None`.
- For target flags and value length, `Some(true)` is changed, `Some(false)` is
  unchanged, and `None` is unavailable. Node detachment cannot be treated as an
  unchanged state because the after-fact was not observed.
- Define `CLICK_EXPECTATIONS`, `FILL_EXPECTATIONS`, and
  `SELECT_OPTION_EXPECTATIONS` beside the existing `ACTION_*` constants and set
  every other action's slice to `&[]`. The operation declaration remains the
  only growing action/expectation registry.
- Evaluate only the first target-matching declaration. `AnyObservedRole`
  requires `Some(role)`; it is not a fallback for a missing role.

**Error paths**:

- Evaluation is total and cannot fail. Missing or inconsistent observation
  availability produces `NotEvaluated`; it never turns a dispatched action
  into an error.
- Serde rejects unknown persisted role/note variants through the existing
  current-format decode boundary.

**Acceptance Criteria**:

- [ ] Registry metadata expresses the complete v1 vocabulary without a second
  table or handler-specific branch.
- [ ] Any positive alternative channel yields `Held`; only a complete set of
  unchanged required channels yields `DidNotHold`.
- [ ] Missing role, `None`, every unobserved target outcome, or an absent
  side-channel block cannot produce a negative note.
- [ ] The evaluator returns at most one typed note with the exact approved
  wording.

---

### Unit 2: Carry role provenance from the resolved snapshot binding

**Files**: `crates/krometrail-cdp/src/control/snapshot.rs`,
`crates/krometrail-cdp/src/control/interaction.rs`,
`crates/krometrail-core/src/browser/interaction.rs`

```rust
struct NodeBinding {
    backend_node_id: i64,
    expectation_role: ExpectationTargetRole,
}

pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    pub(crate) document_quad: [f64; 8],
    pub(crate) facts: NodeStateFacts,
    pub(crate) expectation_role: Option<ExpectationTargetRole>,
}

async fn resolve_backend_node(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    backend_node_id: i64,
    expectation_role: Option<ExpectationTargetRole>,
    requirement: ReferenceRequirement,
) -> Result<ResolvedNode>;

impl InteractionRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: InteractionId,
        context: ObservationContext,
        dispatch_time: SessionTime,
        live_observation_time: SessionTime,
        action: BrowserOperationKind,
        sanitized_parameters: SanitizedParameters,
        locator: LocatorSummary,
        target_role: Option<ExpectationTargetRole>,
        outcome: InteractionOutcome,
        postcondition: InteractionPostcondition,
        parent_batch: Option<InteractionId>,
    ) -> Result<Self>;

    pub fn refresh_expectation_note(&mut self);
}
```

**Implementation Notes**:

- Capture the role classification when the AX decoder creates an actionable
  `NodeBinding`. Reference resolution passes it through both the initial and
  post-scroll re-resolution. Selector resolution explicitly passes `None`;
  coordinate and target-wide resolutions already have no node.
- Add `target_role: Option<ExpectationTargetRole>` and
  `expectation_note: Option<ExpectationNote>` to `InteractionRecord` and its
  wire shape. `new` derives the initial note from the current facts.
  `refresh_expectation_note` reruns the same pure evaluator after enrichment.
- The validated wire constructor recomputes the note and rejects a serialized
  note that disagrees with action, role, registry, and postcondition facts.
  This protects the one-authority invariant for current-format data.

**Error paths**:

- A snapshot-less selector remains a valid interaction with `target_role:
  None`; it is not an interaction error and simply suppresses expectation
  evaluation.
- A corrupt current-format record whose note contradicts its facts fails the
  existing record decode boundary explicitly. Older v10 cache is removed by
  Unit 4 before any row is decoded.

**Acceptance Criteria**:

- [ ] A reference locator carries the active snapshot node's bounded role
  classification through post-scroll resolution and record assembly.
- [ ] Selector, coordinate, and target-wide actions carry no role and emit no
  role-based note.
- [ ] Record construction and wire decoding cannot retain a note inconsistent
  with the pure evaluator.

---

### Unit 3: Finalize after side-channel enrichment and project one canonical note

**Files**: `crates/krometrail-cdp/src/session/operations.rs`,
`crates/krometrail-mcp/src/response.rs`

```rust
fn finalize_expectation_note(result: &mut BrowserOperationResult) {
    if let Some(record) = interaction_record_mut(result) {
        record.refresh_expectation_note();
    }
}
```

**Implementation Notes**:

- Call `finalize_expectation_note` unconditionally after
  `attach_new_page_facts` and `attach_download_facts` opportunities and before
  `persist_result_evidence`. This same path covers standalone interactions and
  batch child interactions.
- In `project_operation`, omit `expectation_note` entirely when the record has
  none; otherwise add the note's `message()` beside the always-on
  `postcondition`. Expanded/full record echoes serialize the same typed note
  and role; the projector does not re-evaluate policy.
- Side-channel timeout or authority absence keeps its `Option` absent and the
  evaluator returns `NotEvaluated`. No enrichment or projection failure changes
  the proven dispatch outcome.

**Error paths**:

- Finalization is synchronous and infallible. Existing side-channel degradation
  remains non-error and suppresses the note.
- A serialization failure continues through the existing
  `ResponseInvariantError`; no new external error code is introduced.

**Acceptance Criteria**:

- [ ] Link notes can be emitted only after navigation, page, and download
  channels are all successfully observed unchanged.
- [ ] The exact same typed note is persisted and rendered in concise,
  expanded, and full interaction results.
- [ ] Results without a note do not gain a null/noisy concise field.

---

### Unit 4: Advance the one current persisted format and qualify the seams

**Files**: `crates/krometrail-store/src/index/schema.rs`,
`crates/krometrail-store/tests/sqlite_schema.rs`,
`crates/krometrail-store/tests/temporal_query_index.rs`,
`crates/krometrail-cdp/tests/verified_interactions.rs`,
`crates/krometrail-mcp/src/response.rs`

```rust
// Version 11: persisted interaction records carry target-role provenance and
// the registry-derived expectation note.
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 11;
```

**Implementation Notes**:

- Update current-version assertions and the incompatible-version matrix so v10
  is rejected and cleared; do not add a migration, optional legacy field,
  default, or dual decoder.
- Extend the existing opaque `record_json` round-trip fixture with a role and a
  complete unchanged expectation whose note survives persistence.
- Add one deterministic CDP interaction test proving reference-role handoff and
  post-enrichment finalization; use a CSS-selector counterpart only if it can
  share the same fixture cheaply to prove suppression.
- Extend the existing MCP detail-progression test to assert the concise
  sentence and equality with the expanded/full record note.

**Acceptance Criteria**:

- [ ] Exact v11 stores reopen without writes; v10 is incompatible recording
  cache and only the allowlisted cache members are cleared.
- [ ] Persisted role and note round-trip with the complete postcondition facts.
- [ ] Live deterministic interaction and MCP projection tests prove the two
  cross-package seams without adding browser observation work.

## Implementation Order

1. Add the pure core vocabulary, registry declarations, evaluation truth table,
   and record invariants.
2. Carry the snapshot-derived role into record assembly.
3. Finalize after side-channel enrichment and project the persisted note.
4. Bump the current store format to v11 and update persistence/integration
   qualification.
5. Run focused package tests, then the workspace formatting, schema, check,
   test, and clippy gates.

## Simplification

- Extend `ActionDefinition`; do not create a second expectation registry,
  per-tool match, sensitivity setting, or configurable wording surface.
- Reuse the active snapshot binding, existing postcondition facts, and existing
  session enrichment seam. Add no role query, post-action probe, delay, retry,
  event stream, or persistence migration.
- Keep `navigation_lifecycle_observed`, attempt signals, and all raw facts as
  observations; the evaluator does not overload them into stronger outcome
  claims.
- The five note sentences are centralized on `ExpectationNote::message`; tests
  assert the public wording once rather than duplicating strings across CDP and
  MCP layers.
- No child stories or cleanup stories are warranted; the change is one
  single-stride contract update with one test matrix.

## Testing

The core evaluator receives the only exhaustive unit matrix because the
expectation-by-availability gate is the complex logic:

| Required channel states | Evaluation | Note |
| --- | --- | --- |
| at least one `Changed`, others any state | `Held` | none |
| every required channel `Unchanged` | `DidNotHold` | selected rule's sentence |
| none changed, at least one `Unavailable` | `NotEvaluated` | none |
| required role unavailable | `NotEvaluated` | none |
| action/role has no declared expectation | `NotApplicable` | none |

The matrix is instantiated for the three-channel link alternative and each
single-channel state rule, including `TargetNodeOutcome` degradation,
page/download `None`, empty observed inventories, omitted observed entries, URL
positive fallback, and first-match priority. Stable seam coverage is limited to
one role-propagation/finalization CDP test, one current-format store round trip,
and the existing MCP detail-progression projection test. No test is needed for
the trivial static sentence accessor beyond the projection assertion, and no
existing test should be removed.

## Risks

- **Riskiest assumption — accessible role continuity**: the resolved reference
  must still correspond to the active snapshot binding after scroll. The
  existing authority revalidation already enforces identity; carrying its
  copied role is safe. Fallback: leave the role `None` and suppress the note,
  never infer it from the DOM tag or run a new probe.
- **False negatives from unavailable channels**: popup reconciliation timeout,
  absent download authority, probe failure, node replacement, selector use, or
  missing committed-navigation signal suppresses a potentially useful note.
  This is the intended conservative failure mode; retained evidence and waits
  remain the escalation path.
- **Deferred effects**: a complete unchanged observation point can precede an
  async application effect. Observation-point wording, no failure terminology,
  and one-note cap limit the claim; v1 adds no sensitivity control.
- **Fill length is a coarse fact**: a same-length replacement produces an
  accurate unchanged-length note even when the value changed. The wording is
  deliberately about length, not value or success.
- **Select-option evidence gap**: the landed native `<select>` probe generally
  cannot produce a selected delta, so this rule normally remains
  `NotEvaluated`. Broadening the probe is outside this feature; emitting a note
  from dispatch success or value length would be a stronger unsupported claim.
- **Persisted-shape blast radius**: adding role/note fields without a version
  bump would make v10 rows decode ambiguously. The mandatory v11 bump and
  current-cache replacement are part of the same implementation unit and are
  not optional follow-up work.

## Implementation notes

- Execution capability: inline implementation; this feature was cohesive and
  was implemented against the current main-branch seams.
- Landed the registry-declared expectation vocabulary and typed completeness
  gate in `krometrail-core`, including the exhaustive three-channel link truth
  table and single-channel role/fact rules. `InteractionRecord` now carries
  the resolved target role and at most one typed expectation note.
- Propagated the role from the active accessibility snapshot binding through
  reference resolution and interaction execution. Selector and coordinate
  actions explicitly carry no role. Notes are finalized in `execute_operation`
  after page/download side-channel enrichment and before persistence.
- Added canonical MCP concise/expanded/full projections, current-format store
  round-trip coverage, and the deterministic CDP reference test for an
  unchanged checkbox click. Updated the recording schema from v10 to v11 and
  its version assertions.
- Adaptation from the design: the current tree required extending
  `NodeBinding`/`ResolvedNode` and updating the recent `InteractionRecord::new`
  seam directly; no compatibility aliases, migrations, or extra probes were
  added. The existing selector real-Chrome test remains role-less.
- No new real-Chrome qualification was named by this design, so no opt-in
  browser run was performed.
- Verification: `cargo fmt --all -- --check`; `bash
  scripts/check-wire-enum-schemas.sh`; `CARGO_TARGET_DIR=/tmp/krometrail-target
  cargo check --workspace --all-targets --locked`; `CARGO_TARGET_DIR=/tmp/krometrail-target
  CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --workspace --all-targets
  --locked` (all targets passed with zero failures); and `CARGO_TARGET_DIR=/tmp/krometrail-target
  CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets
  --locked -- -D warnings` all passed. The temporary target directory was used
  because the environment's default Cargo target path is read-only.

## Review adjudication (standard weight, fresh-context Opus, one pass)

Verified clean: completeness-gate truth table (no note with any unavailable
required channel), any-positive-satisfies, structural no-fall-through and
one-note cap, single registry table, closed role authority (selector/
coordinate structurally role-less), seam placement in code, off-seam record
consistency, v11 persistence, bounded wording/privacy, projection scope.

Accepted findings, routed to a follow-up fix batch (closure is
fix-verification only):
1. (significant) Seam-ordering and the link rule are untested on an executed
   path — the only note test (checkbox) has all channels available before the
   seam; deleting `finalize_expectation_note` would pass the suite. Add the
   scripted link-click pair: `NavigationOutcomeUnobserved` with observed-
   unchanged channels + empty reconciliation, and note `None` when
   reconciliation fails.
2. (design-consistent) The evaluator ignores `SideChannelSignals`: a link
   click with `window_open_attempts > 0` and reconciled-empty pages emits the
   note, contradicting the binding side-channel risk note ("attempts with
   empty pages must not read as contradiction"). Observed attempts demote the
   NewPage/Download channels to unavailable → not-evaluated.
3. (minor) Wire decode accepts `target_role` on non-reference locators (the
   store fixture exploits it) — reject at the wire closure; fix the fixture.
4. (minor) The note-mismatch decode guard is untested — add flipped/forged
   note rejection cases.
5. (minor) The pre-existing checked-delta deterministic test was repurposed —
   restore it as its own case; add one-line assertions for selector/
   coordinate `target_role: None`/note suppression and concise key omission.
6. (nit) Comment on the expectation tables that edits require a schema bump;
   dedupe the double `bindings.get` lookup.

Rejected (no action): already-selected radio emits `CheckedStateUnchanged` —
inherent to the declared rule; wording stays observational.

## Review fixes

- Finding 1: added the scripted managed-session reference-link pair. The
  observed unchanged main-frame navigation, empty reconciled page inventory,
  and empty active download authority persist and project
  `NavigationOutcomeUnobserved`; a scripted `Target.getTargets` failure leaves
  the page channel unavailable and persists no note. The persisted record is
  retained by the evidence fake in both cases.
- Finding 2: positive `window_open_attempts` demotes the new-page channel and
  positive `download_requests` demotes the download channel to unavailable.
  The truth table covers empty-attempt suppression, navigation-positive hold,
  download-attempt suppression, and the unchanged no-attempt note.
- Finding 3: wire decoding now rejects target-role provenance on every
  non-reference locator; the store round-trip fixture now uses a reference.
- Finding 4: wire tests cover both inserting a note where none is derivable
  and replacing a derived note with the wrong note kind.
- Finding 5: restored the deterministic checked false-to-true case, retained
  the unchanged checkbox case, asserted selector/coordinate role and note
  suppression, and asserted concise omission of a note field when absent.
- Finding 6: documented the registry-to-record schema dependency and removed
  the duplicate active-binding lookup.
- The persisted format advances from v11 to v12 because attempt demotion
  changes registry-derived note consistency at decode; v11 records are
  incompatible recording cache and are cleared rather than migrated.

## Review closure

Closure verified 2026-07-22: all six accepted findings landed in commit
2e595117 (link-rule seam test pair, attempt-demoted channels, wire role
validation, decode-guard tests, restored positive-delta coverage, registry
coupling comment) with the required v12 schema bump; full gate green;
spot-verified in-tree. Review complete.
