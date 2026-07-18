---
id: epic-agent-browser-ergonomics-temporal-range-handles-authority
kind: story
stage: done
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics-temporal-range-handles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Process-local resolved-range handle authority

## Checkpoint

Add the typed handle identity and one bounded, non-evicting process-local authority that deduplicates exact validated ranges and revalidates every ordered retained source-frame metadata record before resolving a handle.

## Acceptance evidence

- Unit tests prove stable deduplication, distinct identity, capacity behavior, browser-stop survival, session invalidation, and restart/unknown recovery.
- Frame-source doubles prove missing, reordered, or cross-scope metadata fails before a range is returned.
- Composition tests prove MCP dependencies share the one authority built from the root ID source and recording store.

## Ordering

This authority must exist before any public handle-or-range route can be wired.

## Implementation notes

- Execution capability: direct inline implementation; the typed identity, inward port, process authority, and root composition formed one cohesive dependency slice.
- Review weight: standard (project default); review applies at the integrated feature boundary, not this child checkpoint.
- Files changed: `crates/krometrail-core/src/ids.rs`, `crates/krometrail-core/src/range_handle.rs`, `crates/krometrail-core/src/lib.rs`, `src/range_handles.rs`, `src/main.rs`, `src/app.rs`, `crates/krometrail-mcp/src/config.rs`, `crates/krometrail-mcp/src/server.rs`.
- Tests added/removed: added typed-ID round-trip coverage through the existing registry, range deduplication, capacity/collision, unknown/invalidation, missing/cross-scope/reordered metadata, and composition identity tests; removed none.
- Simplification: one process-local non-evicting table performs all handle identity and retained-metadata validation while existing temporal services remain range-based.
- Discrepancies from design: no public session-deletion route currently composes retention deletion, so `invalidate_session` is implemented and tested but has no deletion callback to wire; all lookups still revalidate retained evidence and browser stop intentionally leaves the table intact.
- Adjacent issues parked: none.

## Verification

- `cargo test --bin krometrail range_handles::tests --locked` (4 passed).
- `cargo test -p krometrail-core ids::generated_contract_tests --locked`.
- `cargo test --bin krometrail app::tests::doctor_is_discovery_only --locked`.
- `cargo check --workspace --all-targets --locked`.
