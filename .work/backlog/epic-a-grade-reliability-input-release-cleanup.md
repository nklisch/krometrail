---
id: epic-a-grade-reliability-input-release-cleanup
kind: feature
stage: backlog
tags: [browser, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Release dispatched keyboard state after errors and cancellation

## Outcome and priority

Errors after modifier/key down can return before matching release, including cancellation. Subsequent browser input may inherit stuck state.

- **Priority:** P2 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced missing cleanup path; no live stuck-key incident reproduced during review.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/control/keyboard.rs:164 — fallible chord sequence before release loop
- crates/krometrail-cdp/src/control/keyboard.rs:357 — keyDown/keyUp pair

## Acceptance criteria

- [ ] Inject failure and cancellation at every chord-dispatch boundary and verify best-effort bounded release of input already dispatched.
- [ ] Cleanup has its own bounded allowance rather than immediately inheriting an already-cancelled request; it never replays the requested action.
- [ ] Preserve the primary error and report incomplete cleanup without falsely claiming a known-clean browser state.
- [ ] A live fixture verifies subsequent typing/shortcuts after interrupted chords. Inspect shared pointer/button cleanup for the same category and either cover it or record a separate evidenced finding.

## Implementation direction and boundaries

Track dispatched input state explicitly. Do not build a generic rollback that assumes state-changing browser actions are reversible.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
