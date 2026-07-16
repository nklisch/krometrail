---
id: refactor-centralize-temporal-observation-row-decoding-step-1
kind: story
stage: done
tags: [refactor, storage]
parent: refactor-centralize-temporal-observation-row-decoding
depends_on: []
release_binding: 1.0.0
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Reuse the temporal observation row decoder

## Checkpoint

Centralize the exact seven-column SQLite `RawObservation` decoder across the temporal index modules without changing behavior. Make `crates/krometrail-store/src/index/timeline.rs::raw_observation` `pub(crate)`, use it for `TimelineStore::range`'s `query_map` callback, and import/use it for both `query_row` callbacks in `crates/krometrail-store/src/index/range.rs`.

## Files and symbols

- `crates/krometrail-store/src/index/timeline.rs::raw_observation`: visibility only; preserve the signature, selected-column order, and all `row.get` calls.
- `crates/krometrail-store/src/index/timeline.rs::TimelineStore::range`: replace only the inline decoder callback with `raw_observation`.
- `crates/krometrail-store/src/index/range.rs::SqliteIndex::observation_for_payload`: replace only its inline `query_row` callback and import `raw_observation` instead of `RawObservation`.
- `crates/krometrail-store/src/index/range.rs::SqliteIndex::latest_observation`: same callback substitution.

## Current to target

```rust
// Current in all three query sites
|row| {
    Ok(RawObservation {
        session_id: row.get(0)?,
        target_id: row.get(1)?,
        session_time: row.get(2)?,
        source_time: row.get(3)?,
        observed_time: row.get(4)?,
        kind: row.get(5)?,
        payload_json: row.get(6)?,
    })
}
```

```rust
// Target
pub(crate) fn raw_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RawObservation> { /* existing body unchanged */ }

// Pass the function item to the existing query APIs.
raw_observation
```

## Acceptance evidence

- The only `RawObservation { ... }` construction in `index/timeline.rs` and `index/range.rs` is the shared `raw_observation` function.
- The three existing SQL statements, parameters, predicates, ordering, `LIMIT 1`, `.optional()` behavior, error mappings, validation order, and `decode_observation` paths are unchanged.
- Existing `sqlite_timeline`, `temporal_query_index`, and `temporal_queries` coverage remains unchanged and passes.
- Run `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## Risk and rollback

**Risk**: Low. The callback already decodes this exact selected-column shape; only crate-local visibility and callback ownership change.

**Rollback**: Revert the implementation commit, restore private visibility, and restore the three inline callbacks. No schema, data, public API, or compatibility migration is involved.

## Implementation notes

- Execution capability: baseline inline ownership; the checkpoint is one exact callback substitution across two files.
- `timeline::raw_observation` is now crate-visible and is the sole seven-column `RawObservation` constructor in `timeline.rs` and `range.rs`.
- `TimelineStore::range`, `observation_for_payload`, and `latest_observation` pass the same function item while retaining their SQL, parameters, ordering, optional behavior, error mapping, and decode paths.
- Verification ran in an isolated detached worktree to exclude concurrent artifact-schema edits: Rust 1.85 format, locked all-target workspace check/test, and Clippy with warnings denied all passed.

## Coordination

- This is the sole implementation checkpoint for the feature; it has no dependencies.
- Do not edit tests, schemas, docs, artifact/browser-event work, or `.work/bin/work-view`.
