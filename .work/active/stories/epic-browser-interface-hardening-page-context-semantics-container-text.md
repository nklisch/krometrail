---
id: epic-browser-interface-hardening-page-context-semantics-container-text
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-browser-interface-hardening-page-context-semantics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Container-qualified role queries

Allow role queries to qualify unnamed controls by bounded ancestor rendered text without spatial guessing or page-wide overmatching.

## Implementation notes

- Execution capability: inline Rust implementation; the core wire contract and active snapshot registry are a cohesive authority boundary.
- Review weight: standard (default).
- Files changed: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`.
- Tests added/removed: serde compatibility coverage for optional `container_text`; registry coverage for distinct unnamed checkboxes and page-root exclusion.
- Simplification: reused existing AX ancestry and semantic metadata rather than adding a locator engine.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-core -p krometrail-cdp --lib --locked snapshot::tests -- --nocapture`
- `cargo test -p krometrail-core --all-targets --locked`

## Review fix

- Review found that generic/page-level AX ancestors can carry propagated sibling text. `container_text` now skips those roles and evaluates only the nearest explicit local container (`listitem`, table/grid, group, article, region, or label-like roles).
- Added a `contains` regression: a checkbox sharing `main` and `generic` wrappers with unrelated sibling text yields `no_match`; distinct Todo-style list-item checkboxes remain uniquely queryable.
