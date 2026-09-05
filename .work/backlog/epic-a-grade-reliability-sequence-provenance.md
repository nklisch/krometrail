---
id: epic-a-grade-reliability-sequence-provenance
kind: feature
stage: backlog
tags: [visual, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Preserve and validate complete temporal sequence provenance

## Outcome and priority

A sequence with five source IDs and decoded indices [1,3] round-tripped to two source IDs, no source indices, and source range 1…3 instead of 0…4. with_source_provenance also accepted [1,3,1]. This undermines the independently published library's evidence authority; the review did not establish that the main runtime uses this exact serde path.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Both silent round-trip loss and nonadjacent duplicate acceptance reproduced through public APIs.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/temporal-vision/src/sequence.rs:215 — serde skips source identity, indices, range
- crates/temporal-vision/src/sequence.rs:310 — adjacent-only source duplicate check
- crates/temporal-vision/src/sequence.rs:414 — reconstruction via new during Deserialize

## Acceptance criteria

- [ ] Serialization/deserialization preserves complete source identities, source indices, source range, decoded frames, annotations, mask, and region for provenance-bearing sequences.
- [ ] Reject all duplicate source IDs, including nonadjacent duplicates, and invalid index/order/identity/range relationships at construction and wire boundaries.
- [ ] Add property-based or generative tests for sparse subsets, tied timestamps, source-range endpoints outside decoded endpoints, duplicate IDs, and annotation ranges; pin whether broader annotations are supported or rejected.
- [ ] Source-derived manifests remain traceable to the same evidence after round-trip. If a deliberately lossy transfer type is needed, name it separately rather than silently changing FrameSequence meaning.
- [ ] Follow the crate's independent versioning/publication contract; maintain one current representation rather than a historical-format migration layer.

## Implementation direction and boundaries

Keep decoded-subset identity separate from full-source identity through all constructors and serializers. Include review improvement #4 here rather than creating a duplicate test-only ticket.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
