---
id: feature-retention-trim-transparency-grace-ordering
kind: story
stage: done
tags: [store]
parent: feature-retention-trim-transparency
depends_on: [feature-retention-trim-transparency-status-transparency]
release_binding: 1.6.1
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Grace as an ordering exception + agent-visible override warning

## Checkpoint

Unit 3 of the parent feature. Make artifact grace real in the common agent
pattern (artifacts derived from the oldest retained window). Design in the parent
body (`## Architectural choice` → "Hollow grace", `## Implementation Units` →
Unit 3).

- `crates/krometrail-store/src/index/retention.rs`: add
  `artifact_grace_since_unix_ms` to `oldest_reclaimable_artifact` so tier 1 skips
  graced artifacts and takes the next-oldest reclaimable object; thin callers pass
  `None`.
- `crates/krometrail-store/src/recording.rs`: thread grace into the artifact tier
  of `reclaim_once`; restructure the override so it wraps *both* tiers — drop grace
  only when nothing else is reclaimable, `under_pressure`, and grace was active,
  setting `artifact_grace_overridden`. Keep the single unified `reclaim` walk and
  absolute pin protection. Latch `grace_override_active` on the store (mirror
  `trim_exhausted`); clear on override-free progress or when back below high-water.
- Surface the override: `generate_artifacts` and the temporal notes read the
  latched flag (shared with the temporal-note story) — no bespoke return channel
  through `cleanup_to`.

## Done when

- Fresh artifact + backing segment derived from the oldest window survive pressure
  while an older non-graced object is reclaimed instead.
- Override fires only when nothing else is reclaimable, sets `grace_override_active`,
  keeps the `retention.artifact_grace_overridden` event, and produces an
  agent-visible `RetentionWarning::ArtifactGraceOverridden`.
- Pinned graced segment is never evicted, even in the override path.
- One reclaim walk preserved; durability/journal path untouched.

## Implementation notes

- Added grace-aware artifact and backing-segment candidate filtering, preserving
  the unified artifact → segment/event reclaim walk and the existing deletion
  journal/accumulator barriers. Grace is dropped only for the final pressure
  fallback; the override is latched for status and retains the existing
  `retention.artifact_grace_overridden` event.
- Added `index::retention::tests::artifact_grace_skips_recent_publications_and_keeps_retention_order`.
- Verification: focused store tests passed, including the new grace-order test;
  the final locked workspace gate passed.
