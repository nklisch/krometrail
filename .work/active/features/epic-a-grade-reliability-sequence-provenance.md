---
id: epic-a-grade-reliability-sequence-provenance
kind: feature
stage: implementing
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

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Both silent round-trip loss and nonadjacent duplicate acceptance reproduced through public APIs.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized for the bounded checkpoint/design below after the user asked to continue execution. No release or paid model-effectiveness qualification is authorized.

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

## Authorized design and implementation boundary — 2026-09-05

The user authorized continued execution. Preserve the entire validated sequence through one current serialized representation: decoded frames, annotations, region/mask, full source identities, subset indices, and declared source range. Deserialize through the same constructor/validation authority rather than installing unchecked fields or silently discarding provenance. Reject nonadjacent duplicate source IDs without strengthening generic identifier trait bounds merely for an implementation convenience. Validate source index/order/identity/range relationships and pin the treatment of annotations outside decoded endpoints but inside source range; current supported constructors and generators must stay coherent.

Keep public generic identifier behavior, source-versus-decoded authority, and independent crate release ownership. No migration/legacy reader and no version bump/publication in this task. Scope code to temporal-vision sequence constructors/serialization and its relevant tests/docs; do not edit root Cargo/lock/release tooling shared with the distribution owner. Use deterministic generative or property-style coverage without introducing a dependency unless justified: sparse subsets, tied timestamps, full-range endpoints, duplicate IDs, malformed wire relationships, annotations, masks/regions, and manifest equivalence across round-trip. Demonstrate actual red-to-green reproductions for data loss and duplicate acceptance. Parent review and integration gates precede acceptance.
