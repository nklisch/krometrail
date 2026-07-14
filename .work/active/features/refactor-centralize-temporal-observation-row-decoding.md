---
id: refactor-centralize-temporal-observation-row-decoding
kind: feature
stage: done
tags: [refactor, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize temporal observation row decoding

## Refactor Overview

The temporal index has three exact copies of the same seven-column `RawObservation` decoder: the two `query_row` callbacks in `index/range.rs`, plus the `query_map` callback in `TimelineStore::range` in `index/timeline.rs`. The existing private `raw_observation` function in `timeline.rs` is already used by timeline replay checks and is the correct single owner.

This is a high-value, low-risk pure refactor: one crate-private decoder makes the selected-column order auditable and prevents timeline, replay, and anchor reads from drifting. It changes no SQL, validation, error mapping, ordering, or optional-row semantics.

**Black-box purity**: for every durable timeline row and invalid/corrupt row, `observation_for_payload`, `latest_observation`, and `TimelineStore::range` retain the same observation, `None`, ordering, or persistence error. `pub(crate)` changes only an internal module boundary; no external API changes.

**Scope guard**: direct-read only; no nested agents or peeragent. Implementation must not edit tests, schemas, docs, artifact/browser-event work, or `.work/bin/work-view`.

## Refactor Steps

### Step 1: Reuse one temporal row decoder everywhere

**Priority**: High
**Risk**: Low
**Source Lens**: elimination / missing abstraction (duplicated domain mapping)
**Files**: `crates/krometrail-store/src/index/timeline.rs`, `crates/krometrail-store/src/index/range.rs`
**Story**: `refactor-centralize-temporal-observation-row-decoding-step-1`

**Current State**:

- `timeline.rs::raw_observation` is private and is used by replay validation; `TimelineStore::range` independently constructs `RawObservation` in its `query_map` callback.
- `range.rs::SqliteIndex::observation_for_payload` and `latest_observation` independently construct the same seven fields in their `query_row` callbacks.
- Every copy reads columns in this exact order: `session_id`, `target_id`, `session_time_be`, `source_time_be`, `observed_time_be`, `kind`, `payload_json`.

**Target State**:

```rust
// crates/krometrail-store/src/index/timeline.rs
pub(crate) fn raw_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RawObservation> { /* existing seven row.get calls unchanged */ }

// TimelineStore::range query_map callback
raw_observation
```

```rust
// crates/krometrail-store/src/index/range.rs
use super::{
    SqliteIndex, codec,
    timeline::{decode_observation, raw_observation},
};

// Both existing query_row calls
raw_observation
```

No `RawObservation { ... row.get(...) }` construction remains outside `timeline.rs::raw_observation` in the temporal index modules.

**Implementation Notes**:

- Change only `raw_observation` visibility from private to `pub(crate)`; keep its signature and all seven `row.get` calls unchanged.
- Replace the `TimelineStore::range` `query_map` closure with the function item `raw_observation`.
- In `range.rs`, replace both inline callbacks with the imported `raw_observation`; remove the now-unused `RawObservation` import.
- Keep the `observation_for_payload` and `latest_observation` SQL text, parameters, predicates, sort directions, `LIMIT 1`, `.optional()`, error strings, pre-query marker/navigation validation, and `decode_observation` calls unchanged. Keep `TimelineStore::range` preparation/query/read error mapping and final decode order unchanged.
- The existing replay-check call sites in `append_observation_tx` continue using the same decoder and require no semantic edits.

**Acceptance Criteria / Test and Gate Evidence**:

- [ ] `raw_observation` is the sole `RawObservation` row construction in `index/timeline.rs` and `index/range.rs`; `range.rs` supplies it to both anchor `query_row` calls and `timeline.rs::range` supplies it to `query_map`.
- [ ] A diff confirms all three SQL statements, query parameters, ordering, optional behavior, error messages, validation order, and `decode_observation` paths are unchanged.
- [ ] Existing `crates/krometrail-store/tests/sqlite_timeline.rs`, `temporal_query_index.rs`, and `temporal_queries.rs` coverage remains unchanged and passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

**Rollback**: Revert the one implementation commit, restore `raw_observation` to private, restore the `RawObservation` import and all three inline callbacks. No schema, data, migration, public API, or compatibility rollback is required.

**Atomicity**: The visibility and call-site substitutions form one small, buildable step. There is no public API or schema migration and no irreversible operation.

## Alternatives Rejected

- **Extract a new helper in `range.rs`**: rejected because it would leave the third identical decoder in `TimelineStore::range` and split ownership away from the existing replay decoder.
- **Add a trait/generic row-decoding abstraction**: rejected as speculative indirection for one concrete SQLite row shape; `pub(crate) fn raw_observation` is the shortest clear boundary.
- **Merge or rewrite the SQL queries**: rejected because query scope, ordering, optional semantics, and error mapping are explicitly out of scope.
- **Add or alter tests**: rejected for this structural substitution; existing SQLite/timeline/range tests are the appropriate black-box evidence.

## Implementation Order

1. `refactor-centralize-temporal-observation-row-decoding-step-1` — make the existing decoder crate-visible and route all three temporal index callbacks through it; verify the full Rust quality gate.

## Implementation summary

The one checkpoint landed in `28fe394`. `timeline::raw_observation` is crate-visible and now serves replay validation, timeline range reads, payload-anchor reads, and latest-anchor reads. The three duplicated seven-column closures were removed without changing query text, parameters, ordering, validation, optional behavior, error strings, or domain decoding. Rust 1.85 format, locked workspace all-target check/test, and Clippy with warnings denied passed in an isolated worktree; concurrent artifact work was excluded.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Evidence**: Independent cross-model standard review confirmed one constructor remains, all four read paths select the identical seven columns and route through it, the helper body is byte-identical, and the implementation diff changes only visibility/imports/callback ownership plus substrate bookkeeping. SQL, parameters, ordering, optional behavior, validation, error mapping, decoding, schemas, and tests are untouched. A focused store check passed; the full isolated Rust 1.85 gate was already green. No re-review is required.
