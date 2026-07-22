---
id: epic-state-aware-interaction-results-postcondition-core-projection
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-postcondition-core
depends_on: [epic-state-aware-interaction-results-postcondition-core-fact-capture]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Concise postcondition projection

Checkpoint for Unit 5 of the parent feature's design: the interaction
projection arm in `crates/krometrail-mcp/src/response.rs` attaches
`result["postcondition"]` (serialized from `record.postcondition`) at every
detail level including concise, per the epic's on-by-default decision.
Includes the response-shape test updates and one gated real-Chrome
qualification (checkbox click asserts a checked delta end-to-end).

## Acceptance evidence

- Concise interaction response contains the bounded postcondition block;
  expanded/full record echo carries the same field.
- Response-shape tests updated; no other tool responses change.
- Gated real-Chrome checkbox qualification added.
- Workspace gate green.

## Implementation

Landed as designed:

- `crates/krometrail-mcp/src/response.rs` — the interaction projection arm
  serializes `record.postcondition` into `result["postcondition"]` before the
  detail split, so every level (concise included) carries the block; the
  expanded/full `record` echo keeps the identical field (one authority
  projected twice). No other tool projection changed.
- Response-shape test upgraded: the interaction record fixture now carries a
  populated checked-delta postcondition, and the test asserts the exact block
  shape at concise/expanded/full plus its equality with the record echo.
- Gated real-Chrome qualification
  (`opt_in_real_chrome_checkbox_click_reports_a_checked_postcondition_delta`
  in `crates/krometrail-cdp/tests/verified_interactions.rs`): a `#checkbox`
  click asserts `checked false → true, changed: true`, node `present`, and
  `url_changed: false` end-to-end, cross-checked against the DOM. Skipped by
  default like its neighbors; verified passing locally with
  `KROMETRAIL_REAL_CHROME_TESTS=1` (1.2s), which also qualifies the widened
  probe under Chrome's real side-effect analyzer — the design's top risk.

Verification note: the pre-existing opt-in test
`opt_in_real_chrome_executes_verified_interaction_families` fails identically
at the v1.4.0 release commit on this machine (the fixture's
`window.replaceClickTarget()` read-only evaluation is refused as
side-effecting by the local Chrome) — environment-dependent and unrelated to
this feature; details in the feature's Implementation notes.

Gate: fmt, wire-enum schemas, check, full test suite (1189 passed, 0
failed), clippy -D warnings — all green.
