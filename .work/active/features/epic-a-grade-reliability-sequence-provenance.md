---
id: epic-a-grade-reliability-sequence-provenance
kind: feature
stage: done
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

- [x] Serialization/deserialization preserves complete source identities, source indices, source range, decoded frames, annotations, mask, and region for provenance-bearing sequences. (`crates/temporal-vision/tests/sequence_provenance.rs`: full-fidelity round-trip with byte-stable re-encode, plus subset enumeration.)
- [x] Reject all duplicate source IDs, including nonadjacent duplicates, and invalid index/order/identity/range relationships at construction and wire boundaries. (Adjacent and nonadjacent duplicates at every position; tied/decreasing/out-of-bounds indices; identity and full-source mismatches; range excluding either decoded endpoint; half-declared shape.)
- [x] Add property-based or generative tests for sparse subsets, tied timestamps, source-range endpoints outside decoded endpoints, duplicate IDs, and annotation ranges; pin whether broader annotations are supported or rejected. (Deterministic enumeration: all 31 nonempty subsets of a five-frame source with a tied-timestamp pair × four declared-range shapes — 124 cases — including extension past each endpoint; broader annotations pinned as supported, validated against the declared source range, and constructible through the public `with_provenance` complete-input path and the wire alike — see record below.)
- [x] Source-derived manifests remain traceable to the same evidence after round-trip. If a deliberately lossy transfer type is needed, name it separately rather than silently changing FrameSequence meaning. (No lossy type introduced; manifest regenerated from the round-tripped sequence equals the original, decimated and complete-source cases.)
- [x] Follow the crate's independent versioning/publication contract; maintain one current representation rather than a historical-format migration layer. (Single wire shape, historical payload rejected, no legacy reader; worker did not touch manifests/lock — version recommendation for the parent integration bump is in the implementation record.)

## Implementation direction and boundaries

Keep decoded-subset identity separate from full-source identity through all constructors and serializers. Include review improvement #4 here rather than creating a duplicate test-only ticket.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Authorized design and implementation boundary — 2026-09-05

The user authorized continued execution. Preserve the entire validated sequence through one current serialized representation: decoded frames, annotations, region/mask, full source identities, subset indices, and declared source range. Deserialize through the same constructor/validation authority rather than installing unchecked fields or silently discarding provenance. Reject nonadjacent duplicate source IDs without strengthening generic identifier trait bounds merely for an implementation convenience. Validate source index/order/identity/range relationships and pin the treatment of annotations outside decoded endpoints but inside source range; current supported constructors and generators must stay coherent.

Keep public generic identifier behavior, source-versus-decoded authority, and independent crate release ownership. No migration/legacy reader, no tag, and no publication in this task. Per docs/RELEASING.md, any temporal-vision source change requires its independent semver bump in the same integrated change: this worker must not edit manifests or the lock (reserved to the parent/distribution owner), so the parent integration supplies the required `crates/temporal-vision` version bump, informed by this worker's recommendation recorded in the implementation notes below. This clarification does not waive the version gate or couple the crate version to the product version.

## Implementation record — 2026-09-05 (worker branch `work/sequence-provenance`, base `ba69f404`)

Criteria above are verified by the recorded tests; parent review and integration gates still precede acceptance.

### Design decisions

- **One current wire representation.** `FrameSequence` serializes `source_frame_ids` (required — the historical provenance-dropping payload stays rejected) plus `source_indices` and `source_range` as ordinary optional members with plain `Option` semantics: omission and explicit null are the same value, and both are always emitted as a value or null (no conditional shape). Unknown fields are rejected, and deserialization delegates to the same validating authority as construction. The optional members' all-or-nothing pairing, subset identity and order, and range containment are what deserialize enforces — not member presence.
- **Single construction authority.** A private `assemble(frames, markers, gaps, region, mask, SourceProvenance)` validates decoded frames, then provenance, then annotations against the *effective* range (declared source range when provenance is declared, decoded frame range otherwise), then region/mask. `new` is `assemble` with the decoded ids and no provenance; `Deserialize` and the public `with_provenance` complete-input constructor are `assemble` straight from their inputs; `with_source_provenance` validates the new provenance and revalidates existing annotations through the same shared validators (`validate_source_provenance`, `validate_markers`, `validate_gaps`) without rebuilding the sequence, so reattaching provenance that strands a marker or gap outside the effective range fails with `AnnotationOutOfRange` instead of silently stranding it.
- **All-or-nothing provenance shape.** `source_indices` and `source_range` must be declared together (omission equals null; neither implies the other). The full-source shape (indices `None`) requires `source_frame_ids` to equal the decoded ids exactly; the declared shape (indices `Some`) covers a subset of the retained source or all of it — explicit complete-source indices are valid and let the caller declare a wider source time range — and requires one index per decoded frame, strictly increasing (checked before identity so a reordered subset reports `OutOfOrder` even when its ids also drift), in bounds, and resolving to exactly the decoded frame at that position; the declared range must contain both decoded endpoints.
- **Nonadjacent duplicates rejected without new trait bounds.** Source ids are scanned pairwise with a quadratic slice scan (`Eq` only); `Hash`/`Ord` are not required of caller identifier types for this check.
- **Annotations outside decoded endpoints but inside the declared source range: supported.** They are validated uniformly against `range()` in every path and constructible through the public `with_provenance` complete-input constructor and the wire alike (`new` keeps its useful meaning: annotations validated against the decoded range before provenance exists). Annotations beyond the declared range are rejected everywhere.
- **Specific rejection codes** replace the old single generic `InvalidParameter`: `DuplicateIdentifier` (source ids, with position), `OutOfOrder` (non-increasing indices, with position), `InvalidParameter` with position for bounds/identity, and distinct messages for shape pairing, range containment, and full-source mismatch. `with_source_provenance` no longer needs `F: Clone`; the `Deserialize` impl dropped its `Clone` bound.

### Red-to-green evidence

Red run at base `ba69f404` (old code), `cargo test -p temporal-vision --test sequence_provenance`: 7/7 failed —
- `round_trip_preserves_complete_source_provenance` and `every_decoded_subset_round_trips_with_declared_ranges`: serialized `source_frame_ids` was `Null` — the silent round-trip loss reproduced through the public API.
- `constructor_rejects_duplicate_source_ids_at_any_position`: old code returned `Ok` for source ids `[1, 3, 1]` (adjacent-only duplicate check).
- `wire_rejects_malformed_provenance` and `annotations_validate_against_the_declared_source_range`: every malformed provenance payload silently deserialized with provenance dropped; broader markers were rejected only because the old wire could not express provenance at all.
- `manifests_stay_traceable_across_sequence_round_trip`: manifest regenerated from the round-tripped sequence diverged from the original.
- `constructor_rejects_every_invalid_provenance_relationship`: old code collapsed every provenance error into one generic `InvalidParameter`.

Green after the change: 7/7 pass. Coverage: complete round-trip fidelity (frames, annotations, region, mask, provenance, byte-stable re-encode); duplicate ids at every position on constructor and wire paths; the malformed-wire matrix (legacy payload, duplicates, tied/decreasing indices, out-of-bounds, identity mismatch, full-source mismatch, range excluding either endpoint, half-declared shape, unknown field); deterministic enumeration of all 31 nonempty decoded subsets of a five-frame source (with a tied-timestamp pair) × four declared-range shapes — 124 cases; manifest equality after sequence round-trip (decimated subset and complete-source cases).

### Gates and results

- `cargo fmt --all -- --check` — pass.
- `bash scripts/check-wire-enum-schemas.sh` — pass.
- `cargo test -p temporal-vision --all-targets --locked` — pass; all existing crate tests green, no fixture updates needed.
- `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` — pass after grouping the provenance triplet into a private `SourceProvenance` struct instead of suppressing `too_many_arguments`.

### Version recommendation for parent integration

**0.2.0 (minor), not patch.** The serialized representation changes incompatibly: 0.1.x readers silently drop the new provenance members, and the new reader rejects 0.1.x payloads (required members missing). Under Cargo's 0.x compatibility rules 0.1.x and 0.2.x are distinct ranges, so a minor bump prevents resolvers from silently adopting a wire-incompatible change, while a patch would falsely signal drop-in compatibility for persisted evidence. The tightened `with_source_provenance` rejections (nonadjacent duplicates, stranded annotations) reinforce. The bump belongs to the parent integration commit per docs/RELEASING.md and must not be coupled to the product version.

### Remaining limits

- The wire carries source identity only — not the timestamps of retained-but-undecoded frames — so `source_range` remains a caller declaration validated for containment, not against observed source timestamps. Source populations may exceed the decoded frame count by any amount; no cap relates the two.
- Duplicate detection is O(n²) (deliberate, to keep the `Eq`-only identifier contract; accepted absent measured regression).
- The wire always emits a value or null for each provenance member (one representation, small payload cost).

## Independent review findings and revisions — 2026-09-05

Independent Astra review of `f2d736a9` (reviewer agent5827bc37-007f-4e8; 77 passed / 3 manual benchmarks ignored, plus a field-presence wire probe). Parent adjudicated both required findings valid; revisions below. Correlated corrections folded in.

1. **Constructor coherence (required).** The wire accepted broader annotations (marker at 35, gap 32–38 over decoded 10–30, declared source 0–40) that no in-memory constructor could build — `new` validates against the decoded range before provenance exists. Correction: promoted the provenance triplet to the public `SourceProvenance` bundle (plain input struct, no derives beyond Clone/Debug, no serialized schema) and added the public `FrameSequence::with_provenance` complete-input constructor delegating to the same `assemble` authority as `Deserialize`. `new` and `with_source_provenance` keep their meanings; no renames. Corrected the assemble/reattachment doc comments (`with_source_provenance` calls the shared validators, not `assemble`). Tests: broader marker/gap sequences built through `with_provenance` and the wire agree; reattachment narrowing rejected for both markers (constructor-built) and gaps (wire-built); real generator (`render_difference_map`) renders identical bytes and an identical manifest before and after sequence round-trip — closing the gap that only `ArtifactManifest::from_sequence` was covered. Source-range containment stays a caller declaration; no invented timestamps, no endpoint-equality requirement.
2. **Optional-member presence claims (required).** The record and comments claimed every provenance member is required on the wire; the field-presence probe showed `source_indices`/`source_range` deserialize as ordinary Options (omission = null), and the sparse-only rejection cases passed because of pairing, not presence. Parent chose ordinary Option semantics. Correction: `source_frame_ids` stays required (historical payload still rejected); no presence guards added; claims reconciled in code comments, README, foundation doc, and this item. Tests cover representative omission/null/value combinations on full-source and sparse payloads, not an exhaustive matrix of every combination. Full-source omission/null cases decode to the implicit full-source sequence; invalid sparse cases are rejected by pairing or full-source identity.

Folded-in corrections: "strict subset" wording fixed everywhere — the declared shape covers a subset or all of the retained source (explicit complete-source indices are valid, tested, and used by the runtime); subset enumeration comment fixed from 15 to 31 subsets (124 subset-range cases); regression added with an identifier type that is `Eq` but implements neither `Hash` nor `Ord`, so construction, wire round-trip, reattachment, and duplicate rejection cannot accidentally require stronger bounds; record clarified that O(n²) duplicate detection is deliberate and accepted absent measured regression, and that retained source populations may exceed the decoded frame count without any cap.

Revision red-green: the revised tests against committed `f2d736a9` fail to compile — `temporal_vision::SourceProvenance` unresolved and `FrameSequence::with_provenance` missing — reproducing the absent public construction path; after the revision they pass. temporal-vision version bump remains parent-owned at integration (0.2.0 recommendation unchanged; the public `with_provenance`/`SourceProvenance` addition reinforces minor over patch).

### Authoritative gate receipts — 2026-09-05 revision run

Command form: `flock /tmp/krometrail-reliability-build.lock bash -c 'set -euo pipefail; export CARGO_TARGET_DIR=/storage/cargo-target; ...'`, full output redirected to `/tmp/krometrail-seq-prov-gates.log` (174 lines) and read after exit; success decided by the pipeline-exit code, not output filtering. Exit code 0; every gate marker reached:

- focused `cargo test -p temporal-vision --test sequence_provenance --locked` — `test result: ok. 10 passed; 0 failed`.
- `cargo fmt --all -- --check` — clean.
- `bash scripts/check-wire-enum-schemas.sh` — clean.
- `cargo test -p temporal-vision --all-targets --locked` — all suites green: 44 (lib) + 4 + 4 + 4 + 6 + 2 + 10 (sequence_provenance) + 6 + 6 passed, 3 manual benchmarks ignored, 0 failed.
- `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` — clean.

Honest note on earlier runs: an intermediate gate chain (`sh -c` without `pipefail`) exited 0 and echoed a pass marker while its own log contained `error: could not compile` for a failing fixture assertion and a clippy unused-import; both were real failures, fixed (fixture indices, import removal), and the chain rerun with `pipefail` before this authoritative run. The round-one gate receipts above were produced before the revision and remain accurate for that round. Stash hygiene verified: the round-two stash-swap was popped in the same command that used it; `git stash list` holds only two pre-existing unrelated main-branch stashes, and the working tree contains the intended source (`with_provenance` present, `SourceProvenance` exported).

## Parent integration — accepted and verified

Astra re-review accepted `9470b8b7` with no remaining required P1/P2 findings. Its status-preserving independent run passed 80 crate tests (3 manual benchmarks ignored), formatting, wire checks, and warning-free Clippy. It inspected the corrected authoritative worker log; historical red runs were not independently repeated. The parent corrected the remaining README/foundation wording: null indices/range describe the implicit full-source shape, while explicit indices can cover all source frames with a wider declared range. Coverage claims now distinguish representative optional-member cases from exhaustive enumeration of decoded subsets.

The parent applied the reviewed implementation and correction together and changed only the independent crate version to `0.2.0`. Offline Cargo metadata regenerated the lockfile with exactly that package-version change. Product version remains `1.6.2`; no tag, publication, or release helper was invoked. Main-worktree verification passed with the independent version bump: formatting, wire-schema check, locked workspace/all-targets check and tests, warning-free workspace Clippy, documentation regeneration/build, and diff checks. No browser or paid evaluation was run; ignored/manual qualification remains unqualified.

The first integration run exposed an existing smoke-test readiness race: the test observed the ownership lock before `SqliteIndex::open` created the index, then failed before launching any competing process. The parent repaired test readiness only, waiting under the existing bounded polling budget for the index and segments whose survival is asserted. Five focused repetitions and the complete integration gate then passed using `bash set -euo pipefail` under the shared build lock. Original isolation assertions remain intact. The failed initial run is not counted as a pass; the successful replacement log is `/tmp/krometrail-provenance-integration-gates-2.log`.

This feature is complete at the reviewed source/integration boundary. Product version remains unchanged; the independent library's `0.2.0` source version is not a publication receipt.
