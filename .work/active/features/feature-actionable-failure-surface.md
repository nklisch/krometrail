---
id: feature-actionable-failure-surface
kind: feature
stage: implementing
tags: [agent-ux, browser]
parent: null
depends_on: [feature-temporal-range-artifact-economy]
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Make every failure name its limit and its cheapest recovery

## Brief

Two live shakedowns (2026-07-19) found that failure messages consistently omit
the facts an agent needs to act:

- **Limit errors never state the limit.** Observed: "source read limits exceed
  runtime ceilings", "selected source frame count exceeds the request limit",
  "no exact integer analysis scale fits configured limits", "normalization
  result exceeds configured processing limits" — none carry the ceiling, the
  offending value, or a fitting suggestion. The caller resorts to bisection.
  Contract: every limit rejection reports `(actual, limit)` and, where cheaply
  computable, a suggested value that would succeed.
- **Recovery labels drift from reality.** Observed: `retry: never` +
  `recovery: null` on the CSS-size observation failure that a cross-origin
  navigation fully recovers; "retry once ... safe" on a deterministic node-cap
  failure; "narrow the query to a smaller document" on a scroll (no query
  exists). Audit every failure site: `retry` and `recovery` must reflect the
  cheapest real state change that makes the operation succeed, or honestly say
  none exists.
- **Hard failures drop their evidence.** Failed clicks returned a bare error
  string with no `diagnostics.correlation_id` and no interaction context, so the
  failure cannot be temporally anchored or investigated. Degraded responses keep
  full context; hard errors must too.
- **The evaluate_page refusal classifier never fires on current Chrome.**
  `evaluation.rs` matches the needle "side effect" (space), but Chrome 149 emits
  "Possible side-effect in debug-evaluate" (hyphen), so every refusal presents
  as "page evaluation threw: EvalError ...". Match both forms (or normalize
  hyphens) and add a decode test using the real Chrome 149 description string.

Absorbed backlog: `idea-evaluate-refusal-needle-drift`. Depends on
`feature-temporal-range-artifact-economy` because the limit/recovery audit
rewrites messages in the temporal paths that feature restructures. Implementation
via peeragent Codex `gpt-5.6-luna` per operator decision (2026-07-19).

## Simplification opportunity

A shared bounded-limit-error constructor (actual, limit, suggestion) can replace
the current ad-hoc message formatting at each ceiling; check for existing
`operation_error` helpers to extend instead of adding a parallel path. The
recovery audit may delete recovery prose that restates the error rather than
naming an action.

## Explorer map (verified file:line)

- Error type `KrometrailError` (code/message/context/retry/recovery)
  `crates/krometrail-core/src/error.rs:194-209`; builders `new` L332-341,
  `from_browser_failure` L343-349, `with_retry` L356-359, `with_recovery`
  L361-364; `ErrorCode::default_retry` L245-267, `default_recovery` L269-328;
  `invalid` helper L374-376. Adapter builders: `operation_error`
  `krometrail-cdp/src/control/mod.rs:525-549`, `malformed` L521-523,
  `transport_error` L551-573 ("browser rejected or could not complete the
  page observation command" at L569-571).
- Limit sites with in-scope values:
  (a) "source read limits exceed runtime ceilings" —
  `krometrail-core/src/progressive.rs:396-401` (requested three limits vs
  constants L28-30); (b) "selected source frame count exceeds the request
  limit" — progressive.rs L629-633 (`selected_count` vs `max_frames`);
  (c) "no exact integer analysis scale fits configured limits" —
  `/src/artifacts/generators.rs:360-362` (budget numbers + frame count in
  scope); (d) "normalization result exceeds configured processing limits" —
  `temporal-vision/src/normalize.rs:397-402` (retained_bytes vs
  max_retained_bytes at L388-393); (e) 5000-node messages —
  `krometrail-cdp/src/control/snapshot.rs:846-854` and L513-515;
  (f) tall screenshot — `control/screenshot.rs:389-405` (already carries
  height + recovery); (g) "CSS size must be finite and positive" —
  `krometrail-core/src/browser/observation.rs:163-169` (width/height f64 in
  scope; via `invalid` → InvalidInput, retry Never, recovery None).
- Hard-fail vs degraded: `Projection` machine
  `krometrail-mcp/src/response.rs:375-429`; hard path `visible_error*`
  L705-721 — **does not set `projection.interaction`**, so failed envelopes
  lose the interaction anchor + record; registry Err wiring
  `registry.rs:860-867` and `call_error_result` sites. Diagnostics ARE
  attached to failed envelopes by the server (`attach_diagnostics`
  `server.rs:231-265`, test `server.rs:409-457`) — the real gap is the
  anchor/record, plus protocol-level `Err(ErrorData)` paths where the block
  lands under `error.data.diagnostics` (L212-229).
- Interaction record on failure: `InteractionRecord::new` only at
  `control/interaction.rs:258-268` (post-dispatch); preflight/dispatch errors
  return earlier (target_hidden L293/L359, stale authority L127-131, dispatch
  L162/L168). Post-dispatch observation failures already produce a record
  with unavailable observation (L217-249).
- Evaluation classifier: `evaluation.rs` needle `.contains("side effect")`
  L112-113, refusal message L115-119, throw path L127-151; tests
  `control/tests.rs:30-104`.
- SPEC: "Errors and Degraded Operation" L427-447.

## Design decisions

- **One sized-limit constructor in core**: `KrometrailError::limit_exceeded
  (code, subject, actual, limit, Option<suggestion>)` (exact name/shape up to
  the implementer) rendering "…: {subject} {actual} exceeds limit {limit}"
  plus "try ≤ {suggestion}" when computable. All seven sites adopt it;
  formatting lives once. `NonEmptyText` bounds keep messages compact.
- **Recovery audit is a table, not a rewrite**: each audited site gets an
  explicit (retry, recovery) decision recorded in this body during
  implementation. Known corrections: CSS-size observation failures →
  `after_recovery` + "reload or navigate the page; a cross-origin navigation
  restores observation when a same-origin reload does not" (residual cases
  after feature-window-lifecycle-integrity's fallback); the geometry
  node-cap message loses its query wording on the scroll path (coordinated
  with feature-observation-projection-economy's fallback — after that
  feature the remaining query-time 5000-node omission message keeps "narrow
  the query" because there a query genuinely exists); deterministic limit
  errors are `never` + sized suggestion, not "retry once".
- **Failed envelopes keep the interaction anchor**: `visible_error*`
  populates `projection.interaction` from the error's `ErrorContext`
  (interaction id, target) when present; adapter preflight errors gain
  `with_context` so the anchor is available. No fabricated records: where no
  record exists (never dispatched), none is emitted.
- **Needle fix**: normalize the exception description (lowercase + '-'→' ')
  before the "side effect" match; test with the literal Chrome 149 string
  "Possible side-effect in debug-evaluate".

## Implementation Units

### Unit 1: Sized limit errors
**Files**: `crates/krometrail-core/src/error.rs`, sites (a)-(g) above,
`docs/SPEC.md` (errors section wording)

**Acceptance Criteria**:
- [ ] All seven sites report actual + limit; (a) names which of the three
      limits exceeded which ceiling; (c) includes frame count and budget;
      (g) includes the observed width×height.
- [ ] Suggestion present where computable (e.g. max_frames page size, tall
      screenshot already-good text preserved).

### Unit 2: Recovery-label audit
**Files**: `crates/krometrail-core/src/error.rs` (default tables), adapter
sites per the audit table

**Acceptance Criteria**:
- [ ] Audit table recorded in this item body with per-site (retry, recovery).
- [ ] CSS-size and deterministic-limit corrections shipped per decisions.

### Unit 3: Anchored hard failures
**Files**: `crates/krometrail-mcp/src/response.rs` (`visible_error*`),
`crates/krometrail-cdp/src/control/interaction.rs` (context on preflight
errors)

**Acceptance Criteria**:
- [ ] A preflight-failed click envelope carries `interaction` context
      (target, operation) and diagnostics; a post-dispatch failure carries
      the full record (via lifecycle feature's degradation) — no regression.

### Unit 4: Side-effect needle
**Files**: `crates/krometrail-cdp/src/control/evaluation.rs`,
`crates/krometrail-cdp/src/control/tests.rs`

**Acceptance Criteria**:
- [ ] Chrome 149 literal "Possible side-effect in debug-evaluate" classifies
      as refusal; legacy "side effect" spelling still classifies.

## Implementation Order
1. Unit 4 (independent, tiny)
2. Unit 1 (constructor + sites)
3. Unit 2 (audit, builds on Unit 1)
4. Unit 3 (anchoring)

## Testing
- Unit tests at each limit site asserting actual/limit presence; classifier
  decode tests with real Chrome strings; envelope test that a failed click
  keeps anchor + diagnostics.

## Risks
- Message-text changes ripple into existing assertions; the implementer must
  update tests asserting old strings rather than weakening them.
- Coordination with the other three features on shared sites (CSS-size,
  node-cap) — this feature runs last in the batch to audit final text.
