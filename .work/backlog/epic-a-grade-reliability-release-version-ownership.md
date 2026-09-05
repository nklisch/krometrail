---
id: epic-a-grade-reliability-release-version-ownership
kind: feature
stage: backlog
tags: [infra, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Keep independent crate and all plugin versions correct through release

## Outcome and priority

Product 1.6.2 and temporal-vision 0.1.1 cause the lock verifier to throw "Cargo.lock workspace package temporal-vision did not start at 1.6.2". Separately, the new Antigravity manifest is outside the product version projection inventory. Existing distribution fixtures passed despite the real mixed-version workspace failure.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Release verifier failure reproduced; omitted Antigravity version projection code-traced.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- scripts/bump-version.ts:132,254 — every member enters product-version validation
- scripts/bump-version.ts:152 — derivedVersionPaths omits plugin/plugin.json
- plugin/plugin.json — independently omitted Antigravity projection
- docs/RELEASING.md — temporal-vision release ownership

## Acceptance criteria

- [ ] A hermetic fixture mirroring the current mixed-version workspace successfully bumps the product while leaving temporal-vision and unrelated lock packages unchanged.
- [ ] Every shipped plugin/catalog version, including plugin/plugin.json and plugin/version, equals the resulting Cargo product version; no projection silently drifts.
- [ ] One explicit version-ownership/projection inventory drives updates and validation; fixtures detect a newly shipped unregistered version projection.
- [ ] Dry-run, verifier failure, rollback, and independent-crate release boundaries are tested without network, tags, pushes, or mutation of standalone installations.
- [ ] Retain exact-version managed activation and Cargo as the sole product release authority.

## Implementation direction and boundaries

Distinguish workspace membership from product-version ownership. Combine the two review findings because one release-ownership contract should prevent both.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
