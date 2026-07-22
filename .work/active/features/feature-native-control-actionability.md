---
id: feature-native-control-actionability
kind: feature
stage: done
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Native control actionability

## Brief

Two GitHub issue #14 findings show ordinary semantic interactions failing
against common native controls, forcing CSS fallbacks or low-level choreography:

- **Upload affordance does not resolve to its native input (finding #2).**
  `upload_files` against the visible accessibility reference failed with
  `reference_not_actionable`, while targeting the hidden native file input by
  CSS selector succeeded. The semantic upload affordance should resolve to its
  associated native input (label association, wrapping, or aria linkage), or
  the failure should identify that input as the required target. Correlation:
  `9fbbd5bf-71cd-41b6-bbbf-0d90dd302079`.
- **Native date inputs cannot be filled through ordinary interactions
  (finding #3).** The date field was represented semantically, including
  month/day/year spinbuttons, but `fill` against both the native input selector
  and the structured date reference failed because the backing node was invalid
  for the requested interaction. A normal correction workflow required
  low-level key choreography or DOM evaluation. `fill` should support native
  date/time inputs with a validated value and proper events, or fail with an
  explicit guided path.

Both fit the ergonomic-input-canonicalization pattern: materialize the semantic
affordance into the explicit native authority the browser actually requires,
keeping the convenience form as provenance.

## Simplification opportunity

None identified beyond folding the resolution into the existing actionability
checks rather than adding a parallel pre-flight surface.

## References

- GitHub issue #14, findings 2 and 3 (macOS, authenticated local React app).

## Design decisions

- **Canonicalization triggers only on kind-requirement miss**: the associated-input
  (upload) and shadow-host (temporal segment) resolution runs only after the directly
  resolved node fails its `FileInput`/`Editable` kind check — happy paths keep their
  current single-probe cost, and the convenience form is materialized into the explicit
  native authority exactly when the browser demands it (ergonomic-input-canonicalization).
- **Association search is bounded and deterministic, never spatial**: self, label
  association (`closest('label')`/`.control`, `HTMLLabelElement.control`), descendant
  `input[type=file]`, `aria-controls`/`aria-owns` ids, a file input whose
  `aria-labelledby` names the affordance, then a *unique* file input among the
  affordance's parent's descendants (covers the ubiquitous sibling-hidden-input React
  pattern). Zero or multiple candidates at the last step yields the guided failure —
  no document-wide "nearest" scan, matching the SPEC rule that bounded relationships
  never fall back to spatial proximity.
- **`FileInput` resolution does not require visibility or geometry**: hidden file
  inputs are the dominant real-world pattern and `DOM.setFileInputFiles` acts by
  backend node id, not pointer input. Connectedness, `input[type=file]` kind, and
  not-disabled remain enforced. `display:none` inputs (no box model) become uploadable.
- **No new wire provenance field**: the sanitized interaction record already echoes the
  requested affordance locator, which is the convenience-form provenance; the canonical
  target is where the action landed. A dedicated `resolved_input` field can be added
  later without compatibility cost (current contract discipline) if agent workflows
  prove to need it.
- **Temporal fill validates by browser assignment, not a Rust date parser**: assign via
  the native `HTMLInputElement.prototype.value` setter and compare — the browser's own
  value-sanitization algorithm rejects malformed values (value becomes `""`), which is
  authoritative for `date`/`time`/`datetime-local`/`month`/`week` including leap days.
  Out-of-range but well-formed values (min/max/step) stick and succeed; constraint
  validation remains the application's concern. The native prototype setter (not plain
  `this.value =`) is used so React-controlled inputs observe the change.
- **`append` mode is an explicit guided failure for temporal inputs**: appending to a
  formatted scalar value is meaningless; failing with the expected-format message is
  more honest than silently replacing.

## Architectural choice

Considered:

1. **Fold resolution into the existing actionability path** (`resolve_backend_node` /
   `validate_node_state`), canonicalizing on kind-requirement miss. Optimizes for one
   authority over target validity; no new surfaces. Chosen.
2. A separate pre-flight "resolve upload target" step in `upload.rs` / a new fill path
   in `form.rs`. Rejected: creates a second, parallel actionability surface the brief's
   simplification note explicitly warns against, and duplicates the stale/hidden/
   disabled checks.
3. Snapshot-time association (mark the affordance node with its associated input's
   backend id during DOM decode). Rejected: pays the cost on every snapshot, only helps
   reference targets (CSS-selector affordances still miss), and goes stale between
   snapshot and action.

Option 1 keeps `ReferenceRequirement` the single actionability authority, reuses the
existing probe roundtrip shape, and works identically for reference and CSS-selector
locators.

## Implementation Units

### Unit 1: Requirement-aware resolution policy
**File**: `crates/krometrail-cdp/src/control/snapshot.rs` (plus the two quad consumers)
**Story**: `feature-native-control-actionability-upload-affordance`

```rust
impl ReferenceRequirement {
    /// FileInput acts by backend node id; it needs neither paint nor box geometry.
    pub(crate) const fn requires_visible_geometry(self) -> bool // FileInput => false, others true
}

pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    document_quad: Option<[f64; 8]>,          // None only for FileInput resolution
    pub(crate) temporal_input: Option<TemporalInputKind>, // Unit 3
}
impl ResolvedNode {
    pub(crate) fn geometry(&self, target_id: TargetId) -> Result<&[f64; 8]>
}
```

**Implementation Notes**:
- `resolve_backend_node` skips the `visuallyHidden` rejection, `DOM.getBoxModel`, and
  the zero-area check when `!requirement.requires_visible_geometry()`; `connected` and
  `interactionBlocked` checks stay.
- `ResolvedTarget::Element.viewport_point` becomes `Option<CssPoint>`
  (`crates/krometrail-cdp/src/control/interaction.rs:31`); the midpoint computation at
  `interaction.rs:565` runs only when geometry exists; `ResolvedTarget::point()` errors
  ("interaction requires visible geometry") on `None`. Consumers:
  `interaction.rs:565`, `screenshot.rs:149,168` (both use geometry-requiring
  requirements, so they call `geometry()`/`point()` and keep working).

**Acceptance Criteria**:
- [ ] A `display:none` `input[type=file]` resolves under `FileInput` with no
      `DOM.getBoxModel` call and upload dispatches against its backend node id.
- [ ] Pointer requirements (`Actionable`, `VisibleGeometry`, `Editable`, `Selectable`)
      still reject hidden nodes and zero-area geometry exactly as before.

### Unit 2: Associated-file-input canonicalization (trickiest unit)
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`
**Story**: `feature-native-control-actionability-upload-affordance`

```rust
const ASSOCIATED_FILE_INPUT_FUNCTION: &str = /* ordered probe, first hit wins:
    1. this is input[type=file]                      -> this
    2. this.closest('label')?.control is file input  -> it (covers wrapping label and label[for])
    3. this.querySelector('input[type=file]')        -> it
    4. ids in aria-controls / aria-owns              -> first that is a file input
    5. this.id referenced from a file input's aria-labelledby -> it
    6. unique input[type=file] among this.parentElement descendants -> it; 0 or >1 -> null
   returns the element or null */;

async fn resolve_associated_file_input(
    transport: &dyn CdpTransport,
    scope: &CommandScope,
    target_id: TargetId,
    object_id: &str,
) -> Result<Option<i64>>
```

**Implementation Notes**:
- Hook: in `resolve_backend_node`, when `requirement == FileInput` and the probe
  reports `isFileInput:false`, run the association probe (`Runtime.callFunctionOn`,
  `returnByValue:false`, **`throwOnSideEffect:false`** — Chrome's side-effect analysis
  conservatively refuses selector queries, per the existing comment at
  `snapshot.rs:1610` — probe is read-only by construction), then `DOM.describeNode` on
  the returned objectId for its `backendNodeId`, then re-validate the canonical node
  once with `FileInput` (no further recursion: the canonical node must itself be a
  valid file input or the operation fails).
- Guided failure when the probe returns null:
  `ErrorCode::ReferenceNotActionable`, message
  `"upload_target_not_file_input: element is not a file input and no associated file
  input was found (label association, contained input, aria-controls/aria-owns,
  aria-labelledby, unique sibling input)"`, recovery
  `"target the page's native input[type=file] directly (CSS selector escape hatch) or
  an element associated with it"`.
- The re-resolution after `DOM.scrollIntoViewIfNeeded` (`interaction.rs:512-563`) is
  pointer-only (`require_viewport_point`); upload never hits the
  selector-identity-change guard, so canonicalization does not fight it.

**Acceptance Criteria**:
- [ ] Upload against a visible wrapping-label affordance reference lands
      `DOM.setFileInputFiles` on the wrapped hidden input's backend node id.
- [ ] Upload against a button whose sibling container holds exactly one hidden file
      input succeeds; with two candidate inputs it fails with the guided message.
- [ ] Upload against a plain button with no associated input fails
      `reference_not_actionable` naming the searched associations and the required
      target.

### Unit 3: Native temporal fill
**Files**: `crates/krometrail-cdp/src/control/snapshot.rs`,
`crates/krometrail-cdp/src/control/keyboard.rs`
**Story**: `feature-native-control-actionability-temporal-fill`

```rust
// snapshot.rs (adapter-internal; not a core/wire type)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TemporalInputKind { Date, Time, DatetimeLocal, Month, Week }
impl TemporalInputKind {
    pub(crate) const fn expected_format(self) -> &'static str; // "YYYY-MM-DD", "HH:MM[:SS]", ...
    pub(crate) fn from_input_type(input_type: &str) -> Option<Self>;
}

// keyboard.rs
const FILL_TEMPORAL_FUNCTION: &str = /* function(value){
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
    const previous = this.value;
    set.call(this, value);
    if (this.value !== value) { set.call(this, previous); return false; }
    this.dispatchEvent(new Event('input',{bubbles:true}));
    this.dispatchEvent(new Event('change',{bubbles:true}));
    return true; } */;

async fn fill_temporal(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &FillRequest,
    node: &ResolvedNode,
    kind: TemporalInputKind,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>
```

**Implementation Notes**:
- Actionability probe (`snapshot.rs:1611`): extend the editable input-type regex to
  `text|search|url|email|tel|password|number|date|time|datetime-local|month|week` (so
  `Editable` passes for enabled, non-readonly temporal inputs with no
  `validate_node_state` change) and return the existing `inputType` through to
  `ResolvedNode.temporal_input` via `TemporalInputKind::from_input_type`.
- `keyboard::fill` branches first: `Some(kind)` → reject `FillMode::Append` with
  `ErrorCode::InvalidInput`
  (`"fill_mode_append_unsupported: <type> input takes one complete value"`), then
  `focus` (existing helper) + `Runtime.callFunctionOn` with `FILL_TEMPORAL_FUNCTION`
  via `DOM.resolveNode` on the backend id (mirror of `form.rs` `select_option`).
  `false` return → `ErrorCode::InvalidInput`,
  `"fill_value_invalid: <type> input requires <expected_format>"` — the explicit
  guided path required by the brief.
- Shadow-segment canonicalization (the month/day/year spinbutton references from
  finding #3): on `Editable` kind-requirement miss, one canonicalization attempt —
  `function(){const r=this.getRootNode&&this.getRootNode();const h=r&&r.host;return h instanceof HTMLInputElement?h:this;}`
  (`returnByValue:false`, `throwOnSideEffect:false`) + `DOM.describeNode`; if the host
  differs, re-validate it once under `Editable`. If Chrome's closed UA shadow root
  makes the host unreachable, keep the miss but upgrade the message:
  `"backing node is not valid for the requested interaction; for a native date/time
  field, target the input element itself"`.

**Acceptance Criteria**:
- [ ] `fill` with `2026-07-21` against `input[type=date]` (selector and reference)
      sets the value and dispatches bubbled `input` and `change` events.
- [ ] `fill` with `not-a-date` fails `invalid_input` naming `YYYY-MM-DD`.
- [ ] `fill` in `append` mode against a temporal input fails `invalid_input` with the
      guided message.
- [ ] A spinbutton-segment reference either canonicalizes to the owning input and
      fills it, or fails with the guidance naming the owning input as the required
      target (real-browser qualification decides which branch Chrome permits, and the
      test pins the observed branch).

### Unit 4: Fixture and SPEC alignment
**Files**: `tests/fixtures/browser/verified-interactions/index.html`,
`crates/krometrail-cdp/tests/verified_interactions.rs`, `docs/SPEC.md`
**Story**: both stories (each lands its own slice)

**Implementation Notes**:
- Fixture additions: an upload affordance group — wrapping-label pattern
  (`<label>Upload <input type=file hidden></label>` with visible styled text), a
  sibling-hidden pattern (`<button>Choose file</button><input type=file style="display:none">`
  in one container), an unassociated decoy button; and a native
  `<input type="date">` with a labelled control.
- Qualification tests in `verified_interactions.rs`: upload through the affordance
  reference for both patterns (assert `files.length` via evaluate), guided failure on
  the decoy, date fill success + invalid-value failure through both locator forms.
- `docs/SPEC.md` Interaction section: one sentence each — upload resolves a semantic
  affordance to its associated native file input or fails naming the required target;
  fill supports native date/time inputs with browser-validated values and bubbled
  events. Regenerate `docs/public/llms-full.txt` via `bun run docs:build`.

**Acceptance Criteria**:
- [x] Real-browser qualification passes for both upload patterns and date fill.
- [x] SPEC describes the new behavior; no command examples beyond current `src/cli.rs`.

---

## Implementation Order
1. `feature-native-control-actionability-upload-affordance` — Units 1, 2, upload slice
   of Unit 4.
2. `feature-native-control-actionability-temporal-fill` — Unit 3, temporal slice of
   Unit 4 (depends on the `ResolvedNode`/policy shape from story 1).

## Simplification
- Resolution folds into the existing `resolve_backend_node`/`validate_node_state`
  authority — no parallel pre-flight surface, per the brief.
- `ResolvedNode::geometry()` consolidates quad access behind one checked accessor
  instead of three raw field reads.
- No new wire types, error codes, or schema changes; existing `ErrorCode` variants
  carry the guided messages, so `scripts/check-wire-enum-schemas.sh` is unaffected.

## Testing
- Deterministic (scripted CDP, `control/tests.rs` + `snapshot.rs` test module):
  canonicalization call sequence for upload (probe → describeNode → re-validate →
  `DOM.setFileInputFiles` with the associated backend id); hidden file input passes
  `FileInput` with no box-model call; guided no-association and ambiguity failures;
  temporal fill dispatches `FILL_TEMPORAL_FUNCTION` and maps `false` to the
  `invalid_input` guided error; append-mode rejection; `validate_node_state` accepts
  date-type editables. These protect the canonicalization contract and error paths.
- Real-browser qualification (`verified_interactions.rs`): the two upload patterns,
  decoy failure, date fill success/invalid — protects against Chrome behavior drift
  (UA shadow roots, setFileInputFiles on hidden inputs, value sanitization).
- No test removals identified; existing upload/fill tests remain valid (they target
  the direct file input and text inputs).

## Risks
- **UA shadow-root host traversal may be blocked by Chrome** for date-input segment
  nodes. Fallback is designed in: the guided failure naming the owning input, with the
  qualification test pinning whichever branch Chrome permits.
- **Chrome's side-effect analysis on probe JS**: mitigated by `throwOnSideEffect:false`
  on the read-only association/host probes (precedent: the existing probe comment at
  `snapshot.rs:1607-1610`).
- **React controlled temporal inputs** could ignore direct value writes; mitigated by
  the native prototype setter + bubbled `input` event, verified against a fixture
  control in qualification.
- **Ambiguity policy at the sibling fallback** (unique-input requirement) may reject a
  legitimate multi-uploader container; the guided error names the CSS-selector escape
  hatch, so no capability is lost.

## Implementation notes

- Implemented the upload affordance slice in
  `feature-native-control-actionability-upload-affordance`: FileInput resolution
  now skips hidden/geometry checks while preserving connected, disabled, and
  post-action fact semantics; bounded association canonicalization resolves
  wrapping labels, contained inputs, ARIA relationships, and unique
  sibling-hidden inputs, with explicit ambiguity guidance.
- Implemented the temporal fill slice in
  `feature-native-control-actionability-temporal-fill`: native temporal input
  kinds pass editable actionability, complete values use the browser's native
  setter and bubbled events, append mode is rejected, invalid assignments name
  the expected format, and one editable-host promotion handles Chrome's native
  date spinbutton segments.
- Required reconciliation: the design's earlier `ResolvedNode` shape was
  updated against the landed `facts: NodeStateFacts` and post-action probe path.
  The new optional geometry and canonical backend identities preserve those
  facts, so upload and temporal targets retain precondition and postcondition
  fact capture through the existing path.
- Added deterministic scripted-CDP tests, upload/date fixture coverage, SPEC
  alignment, and generated documentation. Both opt-in real-Chrome
  qualifications passed: upload affordances landed on hidden native inputs;
  date selector/reference fills emitted both events, invalid/append paths were
  guided failures, and the tested spinbutton segment canonicalized successfully.
- Full Rust gates passed for each story commit and this parent closeout. The
  first Feature 4 workspace-test attempt hit `/tmp` exhaustion while linking an
  unrelated store test; only the task-owned target was removed, then the serial
  rerun passed.

## Review adjudication (standard weight, fresh-context Opus, one pass)

Verified clean: pointer-requirement rigor unweakened (FileInput-only geometry
relaxation, all optional-geometry consumers checked), bounded deterministic
association probe (never spatial), guided failures with recovery, no-recursion
re-validation, postcondition-fact reconciliation held for canonicalized
targets, browser-assignment temporal validation, gated tests skipped by
default.

Findings, all accepted, routed to the post-implementation fix batch (closure
is fix-verification only):
1. (significant) `datetime-local` strict read-back comparison rejects
   browser-normalized valid values (seconds elided) with a guided message
   advertising the rejected form — treat rejection as sanitize-to-empty
   instead of strict inequality; add a datetime-local/seconds qualification
   case.
2. (minor) Spinbutton-segment qualification accepts either branch at runtime —
   pin the canonicalization-success branch and assert the owning input's
   value.
3. (minor) No deterministic scripted coverage for the editable-host promotion
   path — mirror the upload canonicalization test shape.
4. (nit) Editable-miss guidance is date/time-flavored for non-temporal misses —
   scope the message.

## Review fixes

- Temporal assignment now treats an empty sanitized read-back as browser
  rejection, while accepting non-empty normalized values; deterministic
  coverage and a gated `datetime-local` seconds case pin the behavior.
- The gated spinbutton qualification now requires Chrome's canonicalization
  success, then asserts the owning date input's value, and fails if no native
  spinbuttons are exposed.
- Added deterministic editable-host promotion coverage for the exact probe,
  one-shot revalidation, `throwOnSideEffect:false`, and `inputType` to
  `ResolvedNode.temporal_input` threading.
- Editable kind-miss guidance is now date/time-specific only for temporal
  probes; other misses retain generic not-editable guidance.

## Review closure

Closure verified 2026-07-22: all accepted findings landed in commit d7b04559
(full gate + docs build + real-Chrome qualifications green) and were spot-
verified in-tree. Review complete.
