---
id: epic-a-grade-reliability-license-distribution
kind: feature
stage: backlog
tags: [infra]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Ship the declared MIT license text with source and packages

## Outcome and priority

Manifests declare MIT but the repository contains no tracked license text. The independently published crate and binary/plugin packaging need a deliberate license distribution contract.

- **Priority:** P2 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Confirmed repository inventory gap; not a legal opinion about downstream use.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- Cargo.toml:25 — MIT declaration
- crates/temporal-vision/Cargo.toml — independently distributed crate
- git tracked-file inventory at eb5b4656 — no LICENSE/LICENCE/COPYING file

## Acceptance criteria

- [ ] Add standard MIT text with the correct copyright owner/year, using repository ownership history rather than AI attribution.
- [ ] Verify the source distribution and independently published crate include the intended license text; inspect cargo package contents without publishing.
- [ ] Check binary/plugin release packaging and include appropriate licensing material or a deliberate documented distribution policy.
- [ ] Inventory bundled/redistributed third-party notice obligations and retain any required notices; do not assume the project's MIT declaration relicenses dependencies.

## Implementation direction and boundaries

Keep license metadata and shipped text aligned. This item does not authorize publishing a crate or tagging a release.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
