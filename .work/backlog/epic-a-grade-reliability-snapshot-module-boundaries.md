---
id: epic-a-grade-reliability-snapshot-module-boundaries
kind: feature
stage: backlog
tags: [browser, refactor, testing]
parent: epic-a-grade-reliability
depends_on: [epic-a-grade-reliability-snapshot-freshness]
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Split snapshot production responsibilities without changing behavior

## Outcome and priority

Snapshot acquisition, reference identity, normalization, and extraction responsibilities share a large production module. Raw total lines overstate the issue because much of the file is tests, but the production boundary is still difficult to review.

- **Priority:** P3 — wave 4 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Maintainability judgment, not a runtime defect.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/control/snapshot.rs — approximately 3650 lines before its main test module at reviewed revision

## Acceptance criteria

- [ ] Map responsibilities and extract cohesive modules around established ownership boundaries, not arbitrary line-count targets.
- [ ] Public requests/results, reference validity, ordering, omission accounting, time semantics, cancellation, and failure behavior remain observably unchanged.
- [ ] Retain and relocate tests with their owning responsibilities; differential or golden fixtures prove behavior preservation.
- [ ] No new generic framework, compatibility aliases, or dead intermediary abstractions. Update module navigation where needed.

## Implementation direction and boundaries

Perform after the snapshot-freshness fix establishes the correct behavior. Any further semantic change gets its own non-refactor item.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
