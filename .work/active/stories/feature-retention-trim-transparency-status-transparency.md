---
id: feature-retention-trim-transparency-status-transparency
kind: story
stage: implementing
tags: [store]
parent: feature-retention-trim-transparency
depends_on: [feature-retention-trim-transparency-census-staleness]
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Retention status signaling substrate + browser_status transparency

## Checkpoint

Unit 2 of the parent feature and the shared wire/schema chokepoint the grace and
temporal-note stories build on. Design in the parent body (`## Implementation
Units` → Unit 2).

- `crates/krometrail-core/src/recording/retention.rs`: add
  `RecordingTrimState { Steady, Trimming }` (serde `rename_all = "snake_case"`);
  extend `RetentionStatus` with `effective_budget`, `live_instances`,
  `trim_state`, `grace_override_active` (struct, `new()`, `Wire`, validated
  `Deserialize`, `empty()`). Validation: `effective_budget <= configured_budget`,
  `live_instances >= 1`; `trim_state` orthogonal to `budget_state`.
- `crates/krometrail-store/src/recording.rs`: fill the fields in
  `status_from_snapshot` from `effective_budget()`, `live_instances()`, the
  high-water threshold, and the latched grace flag (the flag itself lands in the
  grace story; wire a `false` reader here if that story is not yet merged).
- `crates/krometrail-mcp/src/response.rs`: extend `ConciseRetentionStatus` and its
  Concise/Expanded projection; Full path serializes the new core fields directly.
- Update all `RetentionStatus::new` / `::empty` construction sites (server.rs,
  session.rs, ports/mod.rs, response.rs test).

## Done when

- `status()` reports `Trimming` at/above high-water and `Steady` below, with
  `effective_budget == configured / live` and `live_instances == N`.
- `browser_status` Concise and Full surface all four new fields; Full keeps
  `retained_bounds`.
- `bash scripts/check-wire-enum-schemas.sh` clean; schema.rs retention assertions
  updated. Tone informational.
