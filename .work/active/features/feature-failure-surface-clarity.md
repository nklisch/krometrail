---
id: feature-failure-surface-clarity
kind: feature
stage: done
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Name the actual failure cause at the MCP boundary

## Brief

Four failure surfaces observed in a live shakedown return messages that hide the
information the caller needs to recover:

1. **`evaluate_page` conflates causes and erases the exception.** A thrown
   `Error('deliberate test error')` and a refused mutation (`document.title = 'x'`) both
   return the identical string "page evaluation raised an exception or was refused as
   side-effecting". The exception message never surfaces, making in-page debugging blind.
   Distinguish refusal from throw, and include a bounded, sanitized exception summary.
2. **Grouped-constraint schema errors do not name the missing field.** Temporal tools
   (`query_browser_events`, `list_source_frames`, `fetch_source_frames`) effectively
   require `focus_times: []` even when empty; omitting it fails with "tool arguments do
   not match the advertised input schema at $" — a root-path error with no field hint.
   Either make empty collections optional in the wire contract or report the specific
   unmatched constraint element.
3. **Fresh-session navigation failure lacks recovery guidance.** A managed session started
   without `initial_url` has zero pages; `navigate_page` then fails with "selected browser
   page was not found" — no recovery action ("create a page first with create_page") and,
   unlike other failures, no diagnostics block.
4. **Clipboard failure cause is opaque.** "browser denied or did not complete the
   clipboard request" does not distinguish focus loss vs. permission vs. insecure context,
   while the skill instructs the agent to "correct that browser state" — impossible
   without knowing which state.

## Simplification opportunity

All four are projection-boundary fixes: the underlying operations already know the
distinguishing cause (CDP exception details, serde path, empty-session state, clipboard
API error class). Route the existing cause through the validated-wire-contract error shape
instead of adding new error taxonomies; keep privacy bounds (sanitize exception text to a
bounded length, never page content).

## Code map (explorer-verified, file:line)

- (1) `decode_evaluation` (`crates/krometrail-cdp/src/control/evaluation.rs:53`) emits one
  literal for both top-level (:57-63) and nested (:65-71) `exceptionDetails`, discarding
  `.text`/`.exception.description`/`.className`. `Runtime.evaluate` runs with
  `throwOnSideEffect: true, silent: true` (:29-31), so refusal vs throw is distinguished
  by the side-effect marker in `exception.description`. Bounded sanitizer exists:
  `RedactedText::redact` (`crates/krometrail-core/src/browser/privacy.rs:124`,
  `MAX_REDACTED_TEXT_BYTES = 2048`). Test `evaluation_refuses_exceptions_and_oversized_values`
  (`control/tests.rs:30`) currently asserts the exception text is dropped and must be
  revised to assert bounded redaction instead.
- (2) `focus_times: Vec<SessionTime>` is hard-required (no `#[serde(default)]`) on
  `TemporalContextRequestWire` (`crates/krometrail-core/src/timeline/context.rs:189`),
  `BrowserEventDetailRequestWire` (:213), and the debug-bundle wire struct
  (`crates/krometrail-core/src/debug_bundle.rs:263`). The custom
  `deserialize_validated` wrappers (context.rs:332,391 → validation.rs:28) erase the
  serde field frame, so `normalize_argument_path` (`crates/krometrail-mcp/src/registry.rs:977`)
  renders `$`. Message emitters: `invalid_arguments` (registry.rs:962-971) and the
  `response`-side copy (`response.rs:110-134`).
- (3) `resolve_selection` (`crates/krometrail-cdp/src/targets/model.rs:234`, empty case
  :242-247) builds bare `KrometrailError::new(NotFound, ..)` — no recovery, retry, or
  context; `default_recovery` has no `NotFound` arm (`crates/krometrail-core/src/error.rs:269`).
  Correction from the live session report: `attach_diagnostics`
  (`crates/krometrail-mcp/src/server.rs:231`) does add the correlation block to failed
  envelopes — the confirmed defect is the missing recovery/retry/context, not diagnostics.
- (4) `clipboard_response_error` (`crates/krometrail-cdp/src/control/clipboard.rs:214`)
  already distinguishes secure-context/unavailable/focus/permission via named in-page
  errors; the opaque message comes from `clipboard_dispatch_error` (:179), which flattens
  every non-disconnect transport error to "browser denied or did not complete the
  clipboard request" (:190).

## Design decisions

- **Stable codes stay stable**: no new error codes. Refusal vs throw both remain
  `EvaluationFailed`; the message and recovery differ (avoids taxonomy churn, keeps
  registry tests meaningful).
- **Exception echo bound**: thrown-exception summaries route through
  `RedactedText::redact` at 2048 bytes (class name + description), never raw page values.
  Side-effect detection is best-effort on the description marker; when absent, the
  thrown-exception branch is the default (the marker's cross-version stability is
  unverified — see Risks).
- **Schema fix is localized**: add `#[serde(default)]` to the three `focus_times` wire
  fields (empty collection semantics are already "no focus times") and regenerate the
  canonical schema artifacts, rather than rewriting `deserialize_validated` path
  handling. Additionally, both `invalid_arguments`/`invalid_projection` messages append
  the serde error's own bounded description (e.g. "missing field `focus_times`") — serde
  text, not page content, so no privacy concern.
- **No-page recovery text**: "create a page with create_page, or select an existing page
  with select_page" with `RetryAdvice::AfterRecovery` and session context — satisfies the
  SPEC error contract (docs/SPEC.md:419-425) without touching the `default_recovery`
  table.
- **Clipboard honesty over false precision**: dispatch-stage failures genuinely cannot
  distinguish focus/permission/secure-context; instead of pretending, the dispatch
  message names the stage and transport error class ("clipboard script dispatch failed
  before the page could respond") with the visible-focused-page recovery. The response
  path's four-way discrimination is already correct and untouched.

## Implementation Units

### Unit 1: evaluate_page cause separation
**Files**: `crates/krometrail-cdp/src/control/evaluation.rs`,
`crates/krometrail-cdp/src/control/tests.rs`

- In `decode_evaluation`, branch on the side-effect marker in
  `exceptionDetails.exception.description`:
  - refusal → message "page evaluation was refused as side-effecting", existing recovery
    ("use a bounded side-effect-free expression…").
  - throw → message "page evaluation threw: {redacted summary}", recovery "fix the
    expression or handle the thrown error; the summary is bounded and sanitized".
- Redact via `RedactedText::redact(description, MAX_REDACTED_TEXT_BYTES)`.

**Acceptance Criteria**:
- [x] A double returning a thrown `Error('boom')` yields a message containing a bounded
      "boom" summary and no raw stack.
- [x] A double returning the side-effect refusal yields the refusal message with the
      side-effect recovery and no exception echo.
- [x] Oversized exception text is truncated per `RedactedText` accounting.

### Unit 2: focus_times default + named-field schema errors
**Files**: `crates/krometrail-core/src/timeline/context.rs`,
`crates/krometrail-core/src/debug_bundle.rs`, `crates/krometrail-mcp/src/registry.rs`,
`crates/krometrail-mcp/src/response.rs`, generated schema artifacts

- `#[serde(default)]` on the three `focus_times` fields; regenerate canonical JSON
  schemas (digest/byte-equality checked per canonical-json-schema-artifacts).
- `invalid_arguments`/`invalid_projection` append the bounded serde error description
  after the path: "tool arguments do not match the advertised input schema at $:
  missing field `focus_times`".

**Acceptance Criteria**:
- [x] `query_browser_events` with `range_handle` + `filter` + `selection` and NO
      `focus_times` deserializes (empty vec).
- [x] Schema artifacts no longer list `focus_times` as required; digest check passes.
- [x] A deliberately malformed request's error names the offending field when serde
      reports one.

### Unit 3: no-page recovery enrichment
**File**: `crates/krometrail-cdp/src/targets/model.rs`

- The empty-selection arm of `resolve_selection` gains
  `.with_recovery(..create_page/select_page text..)`,
  `.with_retry(RetryAdvice::AfterRecovery)`, and session context.

**Acceptance Criteria**:
- [x] `navigate_page` in a zero-page session returns the recovery action and
      retry-after-recovery advice (existing envelope tests extended).

### Unit 4: clipboard dispatch-stage message
**File**: `crates/krometrail-cdp/src/control/clipboard.rs`

- `clipboard_dispatch_error` non-disconnect arm names the dispatch stage and bounded
  transport error class instead of the blanket "denied" message; keeps
  `InteractionFailed`, `AfterRecovery`, and the visible-focused-page recovery.

**Acceptance Criteria**:
- [x] Dispatch-failure double asserts the new stage-naming message; the four
      response-path classes (`clipboard.rs:411` test) are byte-unchanged.

## Implementation Order
1-4 are independent; suggested order: Unit 2 (unblocks temporal ergonomics), Unit 1,
Unit 3, Unit 4.

## Testing
- Each unit extends the existing co-located test that currently enforces the defective
  behavior (named above) — regression tests by revision, not new suites.
- MCP envelope tests (`server.rs:410`, `response.rs:3101`) guard that shapes stay
  structured; no new real-chrome tier needed.

## Risks
- **Side-effect marker stability**: the CDP refusal description text is not a documented
  contract across the supported Chromium range. Mitigation: the throw branch is the
  default; a missed marker degrades to a thrown-exception message whose redacted summary
  still contains CDP's own refusal text — informative either way. If qualification data
  later pins the marker, tighten the match.
- **Schema regeneration ripples**: making `focus_times` optional changes generated
  artifacts consumed by digest tests; the unit explicitly includes regeneration so a
  stale-artifact CI failure is impossible to miss.

## Implementation Notes

- Evaluation decoding now recognizes the CDP side-effect marker, preserves the stable
  `EvaluationFailed` code, and emits a bounded `EventRedactor`/`RedactedText` exception
  summary without stack data. Tests cover refusal, thrown `Error('boom')`, and oversized
  exception text.
- All three `focus_times` wire fields now default to an empty collection. MCP schema
  generation derives the optional field from those wire types; there are no checked-in
  MCP schema files to regenerate, so the source-driven schema tests verify the result.
- MCP argument/projection errors append a bounded serde description after the normalized
  path. Quoted values in serde diagnostics are redacted after a focused test showed that
  serde can echo invalid input values.
- Empty page selection now carries the requested create/select recovery and
  `AfterRecovery` retry advice. Explicit target selections retain target context; the
  selected-page case retains the structured error context block and the MCP boundary's
  existing session diagnostics correlation.
- Clipboard dispatch failures identify the dispatch stage and stable transport error
  class while preserving the existing recovery, code, and response-path discrimination.
- Focused checks passed with `CARGO_TARGET_DIR=/tmp/krometrail-cargo-target`; the full
  repository verification gates are the remaining completion check.
