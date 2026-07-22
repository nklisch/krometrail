---
id: feature-native-control-actionability-temporal-fill
kind: story
stage: done
tags: [browser, agent-ux]
parent: feature-native-control-actionability
depends_on: [feature-native-control-actionability-upload-affordance]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Fill supports native date/time inputs with validated values

Design checkpoint for Unit 3 and the temporal slice of Unit 4 in the parent
feature body (`feature-native-control-actionability`). Depends on the
`ResolvedNode`/requirement-policy shape landed by the upload-affordance story.

## Scope

- Extend the actionability probe's editable input-type set with
  `date|time|datetime-local|month|week` and thread the probed `inputType` into
  `ResolvedNode.temporal_input: Option<TemporalInputKind>`
  (`crates/krometrail-cdp/src/control/snapshot.rs`).
- `keyboard::fill` temporal branch (`crates/krometrail-cdp/src/control/keyboard.rs`):
  reject `append` mode with a guided `invalid_input`; focus, then set the value via
  the native `HTMLInputElement.prototype.value` setter with browser-side
  validation-by-assignment and bubbled `input`/`change` events; a rejected value
  fails `invalid_input` naming the expected format per input type.
- Shadow-segment canonicalization on `Editable` kind-requirement miss: attempt
  `getRootNode().host` promotion to the owning input; when Chrome blocks the
  traversal, fail with guidance naming the owning input as the required target.
- Fixture native date input and qualification tests (fill success via selector and
  reference, invalid-value guided failure, append rejection); SPEC Interaction
  sentence for temporal fill.

## Acceptance evidence

- Deterministic scripted-CDP tests: temporal dispatch uses the fill-temporal
  function; `false` maps to the guided `invalid_input`; append mode rejected before
  dispatch; date-type inputs pass the `Editable` requirement.
- Real-browser qualification: `input[type=date]` filled with a valid value observes
  the value and both events; invalid value fails with the format guidance; the
  spinbutton-segment branch (canonicalize vs guided failure) is pinned to Chrome's
  observed behavior.
- `docs/SPEC.md` updated; `bun run docs:build` regenerates the public doc.

## Implementation

- Extended the shared actionability probe to recognize date, time,
  datetime-local, month, and week inputs as editable and threaded the adapter
  `TemporalInputKind` plus expected-format metadata through `ResolvedNode`.
- Added the temporal fill path using the native
  `HTMLInputElement.prototype.value` setter, assignment validation, and
  bubbled `input`/`change` events. Append mode fails before focus with the
  guided invalid-input message; rejected browser assignments name the exact
  expected format.
- Added one bounded editable-host promotion for native date/time spinbutton
  references. The real-Chrome qualification observed three ambiguous
  spinbutton segments (`Month Month`, `Day Day`, `Year Year`) and the tested
  segment canonicalized to its owning input successfully.
- Added deterministic setter/invalid/append tests and the native date fixture;
  selector and reference fills, event counts, invalid values, append mode, and
  the spinbutton branch passed in opt-in real Chrome. `bun run docs:build` and
  the full Rust gate passed.
