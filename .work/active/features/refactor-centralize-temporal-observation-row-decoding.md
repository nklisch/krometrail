---
id: refactor-centralize-temporal-observation-row-decoding
kind: feature
stage: drafting
tags: [refactor, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize temporal observation row decoding

## Brief

The SQLite temporal-anchor queries in `crates/krometrail-store/src/index/range.rs` construct the same `RawObservation` from the same seven selected columns independently at lines 156-165 and 195-204. `crates/krometrail-store/src/index/timeline.rs:161-171` already owns the identical row decoder used by timeline queries and replay checks, but it is private, so the range-anchor implementation cannot reuse it.

Make the existing timeline row decoder `pub(crate)` and pass it directly to both range-anchor `query_row` calls. Keep the SQL predicates, ordering, optional-row behavior, error messages, and `decode_observation` path unchanged. This is a structural ownership cleanup only; it must not merge the two anchor queries or alter scope/payload validation.

**Source lens**: elimination / duplicated domain mapping

**Rationale**: removes one duplicated persistence-to-domain mapping and leaves the selected-column order represented by one auditable decoder, preventing timeline and anchor reads from drifting if `RawObservation` evolves.

**Black-box classification**: pure refactor. For every stored timeline row, `observation_for_payload` and `latest_observation` must return the same observation, `None`, or persistence error as before, with identical query scope, ordering, and validation behavior.

## Evidence and target

- `crates/krometrail-store/src/index/range.rs:148-170` — first inline `RawObservation` row mapping.
- `crates/krometrail-store/src/index/range.rs:183-210` — second inline `RawObservation` row mapping.
- `crates/krometrail-store/src/index/timeline.rs:161-171` — existing equivalent `raw_observation` decoder.
- `crates/krometrail-store/src/index/timeline.rs:173-181` — shared `RawObservation` representation.

**Target state**:

- `raw_observation` is `pub(crate)` in `index/timeline.rs`.
- `index/range.rs` imports and supplies `raw_observation` to both `query_row` calls.
- No duplicate `RawObservation { ... row.get(...) }` construction remains in the temporal index modules.

## Acceptance criteria

- [ ] `range.rs` uses the existing `timeline::raw_observation` function for both anchor queries; no second decoder or copied field mapping is introduced.
- [ ] The two SQL predicates, sort directions, `LIMIT 1`, `OptionalExtension` behavior, and existing error messages remain unchanged.
- [ ] Marker/navigation kind and payload validation remains before database access, and returned observations still pass through `decode_observation`.
- [ ] Existing temporal range, timeline, and SQLite schema/index tests pass without weakening or expanding assertions.
- [ ] `cargo fmt --all -- --check`, locked workspace check/test, and Clippy gates pass.

## Risk and rollback

**Risk**: Low. The decoder already serves the same selected column shape in `timeline.rs`; the only new coupling is crate-private reuse. The main risk is accidentally changing a query's selected-column order or replacing one query's error mapping while editing the call sites.

**Rollback**: Revert the implementation commit, restore a private timeline decoder, and restore the two inline range-query closures. No schema, data, public API, validation, ordering, retention, or error contract migration is required.

## Dependencies and coordination

- Dependencies: none. The change is confined to the committed temporal index row shape and does not depend on artifact generation or browser-event behavior.
- Do not edit active artifact-generation or browser-event work, schemas, tests, docs, or `.work/bin/work-view` while implementing this item.
- This item is deliberately separate from the completed MCP projection and reconnect rejection refactors; neither is re-proposed.

## Discovery notes

- **Scope**: committed source diffs in `5d51e28..c1a76f5`, excluding work-item/archive changes, current uncommitted `.work/bin/work-view`, and future/uncommitted artifact/browser-event work. Reviewed the temporal-query core/ports, interaction and operation paths, store schema/migration/index/recording paths, CDP session evidence/operation/reconnect paths, MCP response boundary, root composition, and their temporal/CDP/store tests.
- **Dispatch**: direct-read only; no agents or peeragent.
- **Value**: high-confidence, low-risk elimination of an exact duplicated domain mapping in the newly implemented temporal query path.
- **Implementation shape**: one cohesive two-module edit; no child story is needed at discovery. Keep the feature at `stage: drafting` for the normal refactor-design implementation checkpoint.
