---
id: epic-a-grade-reliability-bounded-registry-lifecycles
kind: feature
stage: backlog
tags: [testing, browser, storage, agent-ux]
parent: epic-a-grade-reliability
depends_on: [epic-a-grade-reliability-event-target-retirement, epic-a-grade-reliability-range-handle-reclamation]
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Audit remaining bounded registries for full retirement semantics

## Outcome and priority

Admission limits need matching retirement, byte-release, replacement-generation, and process/session shutdown behavior. Fixing the two known maps should not leave the same class of bug elsewhere.

- **Priority:** P2 — wave 3 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Category-level investigation motivated by two code-traced capacity leaks; other registries are not presumed broken.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- src/range_handles.rs — known handle ownership issue
- crates/krometrail-cdp/src/events/pipeline.rs — known target lifecycle issue
- crates/krometrail-mcp/src/session.rs — projected snapshot memory
- src/artifacts/ — scheduler and single-flight lifecycles
- crates/krometrail-cdp/src/session/ — session and resource ownership

## Acceptance criteria

- [ ] Create a bounded audit inventory of live registries/caches with owner, admission unit, byte charge, lifetime, invalidation, eviction, and shutdown path.
- [ ] Inspect snapshot projection memory across target/session turnover and other request/session caches; test known realistic churn instead of guessing arbitrary caps.
- [ ] Fault-injection tests pin budget release after success, error, cancellation, invalidation, and replacement; no either-success-or-failure assertions.
- [ ] Reuse the event, handle, and remote-object items for their known scope. File any newly verified defects separately with reproductions; negative findings remain negative, not mandatory rewrites.

## Implementation direction and boundaries

Use a responsibility/lifecycle table and focused tests, not a universal cache abstraction.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `epic-a-grade-reliability-remote-object-lifetime` — related authority/context, not an implicit blocking dependency.
