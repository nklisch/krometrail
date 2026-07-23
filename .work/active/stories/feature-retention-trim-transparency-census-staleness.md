---
id: feature-retention-trim-transparency-census-staleness
kind: story
stage: done
tags: [store]
parent: feature-retention-trim-transparency
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Census staleness fix: reclaimed-abandoned instance must not count as live

## Checkpoint

Unit 1 of the parent feature. Fix the census's monotonic-maximum floor so a lone
restarted server enforces the whole configured budget again, while keeping the
deliberate equal split.

Root cause and design in the parent feature body (`## Architectural choice` →
"Census phantom", `## Implementation Units` → Unit 1). In short:
`crates/krometrail-store/src/instance.rs` — rename `proved_live` →
`last_proven_live`, change the success branch of `live_instances()` from
`fetch_max` to `store` so the fallback floor tracks the last *successfully proven*
count in both directions. Failure branch and the never-enumerated assumption stay.

## Done when

- `live_instances()` descends when a peer departs even across a subsequent
  enumeration failure (new `tests/shared_budget.rs` case: prove 2, drop peer, one
  good count, then break enumeration → still 1).
- Locked outcomes pinned: reclaimed/departed abandoned instance not counted → lone
  survivor reports 1 and gets the full total; two genuinely-live instances still
  split (each reports 2).
- Every existing shared-budget test stays green (`a_failed_census_does_not_widen_a_share`
  with the peer still present still reports 2; `a_census_that_never_enumerated…`
  unchanged).
- Field/module comments updated to the descend-on-proof semantics. No wire change.

## Implementation notes

- Changed `InstanceCensus` to retain the last successfully proven live count and
  fall back to it after enumeration failure; the equal budget split and
  never-enumerated conservative assumption remain unchanged.
- Added `instance::tests::last_proven_live_count_descends_before_a_later_enumeration_failure`.
- Verification: `cargo fmt --all -- --check`; `cargo check -p krometrail-store --all-targets --locked`;
  `cargo test -p krometrail-store --test shared_budget --lib --locked` (45 unit tests and 11 shared-budget tests passed).
