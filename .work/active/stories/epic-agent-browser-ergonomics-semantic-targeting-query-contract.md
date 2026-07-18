---
id: epic-agent-browser-ergonomics-semantic-targeting-query-contract
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-semantic-targeting
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Define the semantic query contract

Add the validated `SemanticQuery`, text-match, request, bounded result, and explicit outcome domain types, then declare `query_page` once in the browser-operation registry with page selection and batch inheritance.

## Acceptance evidence

- Core unit tests cover defaults, normalization, validation bounds, and all result outcomes.
- Generated schema tests prove the four variants and limits while existing operation request shapes remain unchanged.
- `query_page` is read-only, page-scoped, requested-only, batchable, and contributes no standalone image.

## Ordering

This contract checkpoint has no sibling dependency. Query resolution depends on these stable types and the registry route.

## Implementation notes

- Execution capability: direct inline implementation; the contract is a cohesive core/registry boundary with compiler-guided adapter exhaustiveness.
- Review weight: standard (project default); child story checkpoint does not receive independent review.
- Files changed: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/browser/batch.rs`, plus exhaustive compile plumbing in `crates/krometrail-cdp/src/control/mod.rs`, `crates/krometrail-cdp/src/session/evidence.rs`, and `crates/krometrail-mcp/src/response.rs`.
- Tests added/removed: added core boundary tests for semantic normalization, Unicode case handling, validated defaults/bounds, result outcomes, and generated schema variants; removed none.
- Simplification: declared `query_page` once in the browser-operation registry and reused generated page-selection/batch routing instead of creating a parallel route list.
- Discrepancies from design: none; execution intentionally remains fail-closed until the dependent query-resolution checkpoint installs the adapter implementation.
- Adjacent issues parked: none.
- Verification: focused core observation/operation/batch tests and `cargo check --workspace --all-targets --locked` pass.
