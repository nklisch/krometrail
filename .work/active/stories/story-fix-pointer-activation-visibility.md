---
id: story-fix-pointer-activation-visibility
kind: story
stage: done
tags: [bug, browser, agent-ux]
parent: null
depends_on: []
release_binding: 1.0.5
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Let pointer activation visibility settle

## Symptom

React.dev pointer clicks intermittently returned `target_hidden` although the managed page became
visible and focused; selecting the already selected target and immediately retrying did not recover,
while keyboard activation and a later pointer click succeeded.

## Root cause

Pointer preparation activates and fronts a target but samples `document.visibilityState` only once.
Chrome can acknowledge activation before the document visibility state propagates, so the first
transient `hidden` value is incorrectly treated as the final bounded result.

## Fix approach

Poll the read-only visibility probe at a short interval within the existing two-second activation
deadline. Preserve the same specific `target_hidden` result when the target never becomes visible.

## Regression test

`crates/krometrail-cdp/src/control/tests.rs` scripts a hidden response followed by visible and asserts
pointer preparation succeeds after two probes. It failed after the first probe before the fix.

## Implementation notes

- Execution capability: host agent, high reasoning; the fix changes one bounded activation loop and
  preserves the existing public error contract.
- Files changed: CDP pointer preparation and its interaction tests.
- Confirmation: the new settling regression failed before the loop; all three pointer-target
  activation tests pass afterward, including the persistent-hidden failure case. Live React.dev
  reproduction is reserved for the integrated browser qualification pass.
- Adjacent issues parked: none.

## Review

- **Mode:** bounded inline standalone-story review; no independent or cross-model reviewer ran.
- **Verdict:** approve.
- **Correctness:** pointer preparation now waits for Chrome's visibility propagation inside the existing activation deadline; a target that remains hidden still returns the same specific `target_hidden` error.
- **Tests:** transient-hidden, persistent-hidden, and already-visible paths pass, along with the complete workspace suite and real-Chrome interaction qualification.
- **Design and compatibility:** the loop reuses the existing deadline and cancellation-aware CDP path, adds at most a 16-millisecond cancellation delay between probes, and changes no public request or persisted format.
- **Security:** target activation authority and interaction validation are unchanged; no new input, process, filesystem, network, or secret handling was introduced.
- **Findings:** no blockers, important findings, or nits.
