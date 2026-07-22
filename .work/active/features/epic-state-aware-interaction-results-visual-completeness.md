---
id: epic-state-aware-interaction-results-visual-completeness
kind: feature
stage: implementing
tags: [agent-ux, browser, visual]
parent: epic-state-aware-interaction-results
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Visual completeness marker

## Brief

Issue #14 finding #5: an immediate post-action image contained compositor
artifacts (overlapping duplicate cards); the retained temporal evidence (60
gap-free frames) disproved the apparent product defect. Callers need to know
when the immediate image cannot be trusted as settled so they consult retained
evidence before reporting a visual defect.

The signal already exists and is discarded: the post-action double
requestAnimationFrame compositor wait (bounded 250 ms) logs
`browser.compositor.signal_unavailable` via tracing when the signal never
arrives, then captures anyway with no marker on the response. This feature
surfaces that observed state as a bounded visual-completeness marker on the
immediate screenshot — the `EncodedScreenshot` warning surface is the existing
attach point (its only producer today is the tall-screenshot guidance). The
marker states that visual completeness is unconfirmed and points at retained
evidence; it does not judge the pixels.

Does NOT cover: pixel-content analysis of immediate captures (dark-frame or
damage detection). Frame analysis remains the temporal-vision pipeline's
authority; v1 surfaces only what the compositor-readiness path already
observes. If design finds the rAF signal insufficient to explain the reported
artifact class, it may add a bounded, cheap readiness fact — but never a
pixel-analysis pass on the immediate path.

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: independent capability — no dependency on the
  postcondition block; can land in parallel with `postcondition-core`.

## Simplification opportunity

- Replace the tracing-only discard with the surfaced marker — one signal, one
  consumer path; no parallel "compositor health" side-channel.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (visual-completeness marker;
  two-rAF bounded wait)
- `docs/VISUAL-EVIDENCE.md` — evidence classes (immediate vs retained)
- GitHub issue #14, finding #5 (`44b8a67f-e2bb-49eb-9526-5af6bfd745c7`)

## Design decisions

Resolved with judgment under the active autopilot goal (no questions asked;
seams verified by direct reading of `pages.rs`, `interaction.rs`,
`observation.rs`, `screenshot.rs`, `response.rs`, `error.rs`, and the scripted
CDP test support):

- **Marker travels as a return value from `await_compositor_ready`, attached
  at the two post-action call sites** — not threaded through `observe_live`'s
  signature. The compositor wait is a post-action concern; explicit
  `observe_live`/`take_screenshot` paths run no compositor wait and must never
  carry the marker. Alternative (pass a flag into `observe_live` and attach at
  capture) rejected: it widens a shared 6-parameter signature for a semantic
  that belongs to the post-action callers only.
- **New stable `ErrorCode` variant `VisualCompletenessUnconfirmed =>
  "visual_completeness_unconfirmed"`** rather than reusing
  `page_observation_failed`. The marker is the contract; agents must be able
  to distinguish "screenshot captured, settlement unconfirmed" from "the
  observation failed" by stable code alone, without parsing message text.
  Confirmed no checked-in generated artifact enumerates `ErrorCode` strings
  outside `error.rs` (the wire-enum schema guard is a static serde-naming
  check), so the addition is contained.
- **Marker attaches only when the screenshot part is `Available`** — an
  `Unavailable` screenshot already carries its own error, and a
  visual-completeness marker for a missing image is meaningless.
  `blocked_observation` / `unavailable_observation` paths never ran the
  compositor wait or produced an image, so they correctly carry no marker.
- **All non-confirmed outcomes (timeout, transport error, cancellation) map
  to one marker state** — matching the existing single non-success branch in
  `await_compositor_ready`. v1 surfaces observed signal state as one bounded
  fact; distinguishing causes adds surface without changing the caller's
  correct move (consult retained evidence).
- **Degraded response status is intended, not incidental**: screenshot
  warnings already flip an otherwise-successful MCP response to `Degraded`
  via `Projection::degrade_with` (`response.rs:497-500`). This matches SPEC's
  existing sentence ("a missing signal degrades evidence but does not erase a
  proven dispatch or mutation") and is the point of finding #5 — the caller
  must notice. Marker message stays advisory with recovery pointing at
  retained evidence.
- **Tracing warn `browser.compositor.signal_unavailable` remains** as the
  diagnostics channel; the response marker is the contract. One observation
  site, two bounded emissions, no parallel "compositor health" side-channel.
- **No marker on the retained `InteractionRecord`** — retained temporal
  evidence is itself the settled-state authority the marker points to; the
  record is persisted provenance, and the epic binds the marker to the
  immediate screenshot's warning surface only.
- **SPEC rolls forward code-first in this stride** (marker sentence added to
  Current-State Observation); `docs/VISUAL-EVIDENCE.md` needs no change — its
  evidence-class split already carries the immediate-vs-retained semantics.
- **No child stories** — single-stride, tightly cohesive change (one signal,
  one attach point, one doc sentence); no useful intermediate checkpoint.
- **Dispatch rationale**: direct-read only; the explorer-provided seam map was
  verified in source and no distinct unknown remained to justify fanout.

## Architectural choice

Surface the already-observed double-rAF outcome by making
`await_compositor_ready` return the marker (`Option<KrometrailError>`,
`Some` = unconfirmed) and attaching it to the post-action live observation's
available screenshot via the existing `EncodedScreenshot` warning surface.
Zero MCP-layer code: warnings on post-action screenshots already project into
response warnings and degrade status for interactions
(`project_live_observation_part`, `response.rs:1395-1407`), batch steps
(`response.rs:1269-1281`), and batch final observations. Approaches
considered: (a) chosen; (b) thread a readiness flag into `observe_live` —
rejected per Design decisions; (c) a dedicated `visual_completeness` response
field on `LiveObservation` — rejected: the epic binds v1 to the existing
warning surface, and a new field is a schema change with no added
information over a stable-coded warning.

## Implementation Units

### Unit 1: Stable code + warning attach surface (core)

**File**: `crates/krometrail-core/src/error.rs`

```rust
// in the define_stable_enum! ErrorCode block, after ScreenshotFailed:
VisualCompletenessUnconfirmed => "visual_completeness_unconfirmed",
```

- `default_retry`: add `Self::VisualCompletenessUnconfirmed` to the
  `RetryAdvice::Safe` group (a fresh observation is safe and may confirm
  settlement).
- `default_recovery`: `Some("treat the immediate screenshot as possibly
  unsettled; consult retained temporal frames for the settled state before
  reporting a visual defect")`.

**File**: `crates/krometrail-core/src/browser/observation.rs`

```rust
impl EncodedScreenshot {
    pub fn push_warning(&mut self, warning: KrometrailError);
    // with_warning(self, ...) remains; reimplement over push_warning
}

impl LiveObservation {
    /// Attaches to the available screenshot; no-op when the screenshot part
    /// is unavailable (that part already carries its own error).
    pub fn attach_screenshot_warning(&mut self, warning: KrometrailError);
}
```

**Implementation Notes**:
- `LiveObservation` fields are public; the method is a guarded `if let
  ObservationPart::Available(screenshot) = &mut self.screenshot`.
- Compile-check for non-wildcard `match error.code` sites
  (`resources.rs:753`, `timeline/context.rs:1290`, `session/mod.rs:905`,
  `segments/record.rs`) — expected to have wildcard arms; fix any the
  compiler flags.

**Acceptance Criteria**:
- [ ] `ErrorCode::VisualCompletenessUnconfirmed.as_str() ==
      "visual_completeness_unconfirmed"`; serde round-trips the stable name.
- [ ] `attach_screenshot_warning` appends to an available screenshot's
      warnings and is a no-op on an unavailable part.
- [ ] `bash scripts/check-wire-enum-schemas.sh` passes.

### Unit 2: Surface the compositor signal (cdp control)

**File**: `crates/krometrail-cdp/src/control/pages.rs`

```rust
#[must_use]
pub(crate) async fn await_compositor_ready(
    &self,
    transport: &dyn CdpTransport,
    bound: &super::BoundTarget,
    cancel: &OperationCancellation,
    connection_generation: u64,
) -> Option<KrometrailError>; // Some(marker) when the double-rAF signal did not confirm

fn visual_completeness_unconfirmed(
    target_id: krometrail_core::TargetId,
) -> KrometrailError;
```

**Implementation Notes**:
- Marker construction mirrors `tall_screenshot_guidance`'s locality
  (constructor lives beside its observation site):
  `KrometrailError::from_browser_failure(ErrorCode::VisualCompletenessUnconfirmed,
  message).with_context(ErrorContext { target_id: Some(target_id), ..default })`
  — `from_browser_failure` applies the code's default retry and recovery.
- Message (privacy-bounded, no page content): `"compositor readiness was not
  confirmed within the bounded wait; the immediate screenshot may not show
  the settled page state"`.
- Keep the existing tracing warn in the same non-success branch, unchanged.
- In `observe_after_operation_with_geometry`: capture the return value; on
  the successful `ObserveLive` arm, deref the boxed observation, call
  `attach_screenshot_warning` when `Some`, then wrap in
  `ObservationPart::Available`.

**File**: `crates/krometrail-cdp/src/control/interaction.rs` (~line 251)

```rust
let compositor_marker = self
    .await_compositor_ready(transport, &bound, cancel, generation)
    .await;
// on the successful ObserveLive arm:
Ok((BrowserOperationResult::ObserveLive(mut observation), _)) => {
    if let Some(warning) = compositor_marker {
        observation.attach_screenshot_warning(warning);
    }
    observation
}
```

**Acceptance Criteria**:
- [ ] Interactions and post-operation observations (navigation, reload,
      history, dialog-free paths via `observe_after_operation`) attach exactly
      one `visual_completeness_unconfirmed` warning to the post-action
      screenshot when the rAF signal fails, times out, or is cancelled.
- [ ] A confirmed signal attaches nothing; explicit `observe_live` /
      `take_screenshot` tool paths never carry the marker.
- [ ] The failed-observation arms (`unavailable_observation`,
      `blocked_observation`) are unchanged.

### Unit 3: Tests

**File**: `crates/krometrail-cdp/tests/verified_interactions.rs`

- **Unconfirmed path (interface test — protects the new contract)**: scripted
  click where the compositor-position `Runtime.evaluate` gets a
  `push_failure(... TransportError ...)` so the non-success branch runs
  without waiting out the 250 ms timeout. Assert the result observation's
  screenshot part is available with exactly one warning whose `code` is
  `ErrorCode::VisualCompletenessUnconfirmed` and whose recovery text points
  at retained evidence. Note: `Runtime.evaluate` responses share one
  per-method queue — count preceding evaluate calls in the scripted flow to
  land the failure on the compositor call (the existing test at ~line 235
  shows the call-order bookkeeping).
- **Confirmed path (regression guard)**: extend the existing
  compositor-ordering assertion block (~lines 235-250) with
  `assert!(screenshot.warnings().is_empty())` — proves the marker never
  fires on the healthy path (ScriptedCdp's default unscripted response is a
  success object, so existing tests stay green).

**File**: `crates/krometrail-core/src/browser/observation.rs` (inline tests)

- `attach_screenshot_warning` no-op on unavailable part / append on
  available part — only if not already exercised by the cdp test; skip if
  redundant (smallest useful surface).

No MCP test additions: warning→response projection and degraded-status
behavior are code-agnostic and already covered by existing `response.rs`
inline tests (tall-screenshot precedent).

### Unit 4: SPEC roll-forward

**File**: `docs/SPEC.md` (Current-State Observation, the two-rAF sentence at
~line 106)

Extend in place (no "previously" prose): after "a missing signal degrades
evidence but does not erase a proven dispatch or mutation", add one sentence:
"When the readiness signal does not arrive, the post-action screenshot
carries a visual-completeness marker stating that the image is not confirmed
settled and that retained temporal frames are the authority for settled
visual state."

Then regenerate `docs/public/llms-full.txt` via `bun run docs:build` (never
hand-edit).

## Implementation Order

1. Unit 1 (core code + attach surface) — everything hangs on it.
2. Unit 2 (cdp signal surfacing at both call sites).
3. Unit 3 (tests) — alongside Unit 2.
4. Unit 4 (SPEC roll-forward + docs regeneration).

## Simplification

- The tracing-only discard stops being the sole consumer of the compositor
  signal; the surfaced marker becomes the contract with no parallel health
  side-channel (epic's simplification arc satisfied).
- No new response fields, schemas, or MCP code — the existing warning
  projection carries the marker end to end.
- `with_warning` reimplemented over the new `push_warning` (one mutation
  path).
- Nothing found to delete: the warning surface has one other producer
  (tall-screenshot guidance) and both stay.

## Testing

- Scripted unconfirmed-path test protects the new external contract (the
  marker is the fix for issue #14 finding #5).
- Empty-warnings assertion on the healthy path guards against the marker
  firing spuriously (the false-positive failure mode that would train agents
  to ignore it).
- Existing tests `full_page_screenshot_*_warning` and the MCP degraded-status
  tests already cover the shared warning machinery; no duplication.
- Not directly tested: the 250 ms timeout branch and cancellation branch —
  they share the single non-success arm with the transport-error branch the
  test drives; exercising real timers buys no additional branch coverage.

## Risks

- **Spurious degradation on slow-but-healthy pages**: any page missing the
  250 ms rAF window now yields `Degraded` interaction responses, not just a
  log line. Judged intended (SPEC already calls a missing signal degraded
  evidence) and self-limiting: the marker is advisory, retry advice is
  `Safe`, and recovery points at retained evidence. If field noise emerges,
  the bounded fix is tuning `COMPOSITOR_READY_TIMEOUT`, not the marker.
- **Scripted-queue ordering fragility**: the compositor evaluate shares the
  `Runtime.evaluate` response queue with target-resolution evaluates;
  mis-landing the pushed failure would fail a different call. Mitigated by
  asserting on `command_calls()` positions (existing precedent) and by
  ScriptedCdp's default-success for unscripted commands keeping every other
  test unaffected.
- **Derived `PartialEq` on `EncodedScreenshot` includes warnings** — any
  test comparing whole screenshots across the marker boundary would notice;
  none currently do on the unconfirmed path (it required a scripted failure
  that no existing test produces).
- **Exhaustive `ErrorCode` matches**: a non-wildcard match elsewhere would
  break compilation on the new variant; the compiler surfaces these
  deterministically during Unit 1.
