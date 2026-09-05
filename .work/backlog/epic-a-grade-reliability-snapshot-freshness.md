---
id: epic-a-grade-reliability-snapshot-freshness
kind: feature
stage: backlog
tags: [browser, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Separate snapshot reference validity from content novelty

## Outcome and priority

Attachment and document generation identify reference validity, not whether controls, values, labels, geometry, or semantic context changed. Suppressing target details as unchanged on generation equality can hide same-document application changes.

- **Priority:** P1 — wave 1 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced defect; complete dynamic-page reproduction still belongs in implementation verification.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/control/snapshot.rs:619 — begin_snapshot reuses document generation
- crates/krometrail-mcp/src/session.rs:25 — ProjectedSnapshotMemory
- crates/krometrail-mcp/src/response.rs:2192 — concise_snapshot unchanged projection

## Acceptance criteria

- [ ] A same-document fixture changes labels, values, enabled state, target membership, and relevant geometry; the next economical response exposes the changes rather than claiming unchanged content.
- [ ] An actually unchanged bounded projection remains economical, with honest omissions and a working detail path.
- [ ] Stable element references remain stable when valid; navigation and attachment changes still invalidate them correctly.
- [ ] Cover concise, expanded, full, and post-action/batch projections. Presentation memory is updated only for content actually delivered at the relevant detail/scope.

## Implementation direction and boundaries

Use separate identities for reference lifetime and emitted content novelty. Compare or fingerprint the bounded canonical presentation rather than treating a document identity as a content hash.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
