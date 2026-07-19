---
id: story-align-frame-access-labels
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Align list_frames access labels with actual frame-scope behavior

## Brief

`list_frames` labeled a same-process `srcdoc` iframe (inside a `data:` main document)
`access: "indeterminate"`, yet a frame-scoped `query_page` against that frame's reference
succeeded and returned actionable references. The documented contract says indeterminate
frame scope fails explicitly. Either the access classifier under-reports qualification for
same-process opaque-origin frames (label should be a qualified level), or the query gate
under-enforces (should have rejected). Determine which side is authoritative, align the
other, and cover with a frame fixture test. Behavior observed is better than documented —
prefer upgrading the label over breaking working queries, unless the gate's leniency is
unsound for genuinely cross-process frames.

## Acceptance

- Classifier and query gate agree for: same-origin frame, inherited same-process opaque-origin
  (`about:srcdoc`/`about:blank`) frame, fresh opaque (`data:`) frame, cross-origin out-of-process
  frame, and stale frame reference.
- `list_frames` labels match what a subsequent frame-scoped query actually does.
- Docs/skill text state the final contract.

## Completion notes

The frame-scope resolver is authoritative: it revalidates the live frame tree and process
inventory, rejects detected out-of-process, cross-origin, and fresh opaque frames, and permits a
same-process `about:srcdoc` or `about:blank` child when its opaque origin is inherited from the
parent. `list_frames` uses the same predicate; unavailable process qualification remains
`indeterminate` and fails closed.

- Files changed: `crates/krometrail-cdp/src/control/contexts.rs`,
  `crates/krometrail-cdp/src/control/snapshot.rs`,
  `crates/krometrail-cdp/tests/verified_interactions.rs`,
  `crates/krometrail-cdp/src/qualification_support/static_fixture.rs`,
  `tests/fixtures/browser/browser-contexts/index.html`,
  `tests/fixtures/browser/browser-contexts/cross-origin.html`, `docs/SPEC.md`,
  `docs/ARCHITECTURE.md`, `plugin/skills/krometrail/SKILL.md`, and
  `plugin/skills/krometrail/references/browser-contexts.md`.
- Tests: scripted frame tests cover opaque-origin success, cross-origin and OOPIF rejection;
  the opt-in browser fixture covers same-origin success, opaque/cross-origin rejection, and the
  existing stale-reference rejection after child navigation.
- Verification: focused CDP tests passed; full workspace gates are recorded after the three
  story commits.
- Stage intentionally remains `implementing` per the implementation request; no other work item
  was advanced.

## Review-fix note (2026-07-19)

Opaque-origin equality now qualifies only inherited `about:srcdoc`/`about:blank` children; fresh
opaque URLs such as `data:` are labeled cross-origin and rejected by the same frame-scope gate.
Regression coverage covers both inventory labeling and frame-query qualification.
