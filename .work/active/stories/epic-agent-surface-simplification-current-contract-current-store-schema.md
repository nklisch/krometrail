---
id: epic-agent-surface-simplification-current-contract-current-store-schema
kind: story
stage: done
tags: [storage, infra]
parent: epic-agent-surface-simplification-current-contract
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Replace historical migrations with one current schema

Create one transactional current-v6 schema initializer and exact version validator. Open populated v6 data without writes; reject unversioned non-empty, older, or newer formats before mutation with clear recovery. Prove the consolidated schema retains every current table, column, index, trigger, foreign key, and strict constraint before deleting migration modules and migration-only tests.

## Implementation notes

- Execution capability: high; the change deletes the historical schema authority and therefore required an exact review of the accumulated v6 shape.
- Review weight: standard from the delegated caller; this child closes on focused green evidence and the integrated feature receives review.
- Files changed: `crates/krometrail-store/src/index/schema.rs`, `crates/krometrail-store/src/index/mod.rs`; deleted `migrations.rs` and `schema_v1.rs` through `schema_v6.rs`.
- Tests added/removed: replaced migration-path tests with current bootstrap, exact-current no-mutation, incompatible-version no-mutation, and declarative full-shape assertions for tables, columns, indexes, triggers, foreign keys, strictness, and version.
- Simplification: collapsed seven historical schema/migration modules and upgrade fixtures into one current schema plus initialize-or-validate boundary.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-store --lib schema --locked` and formatting passed; broader crate compilation was temporarily blocked by a concurrent persistence-contract call-site update outside this story.
