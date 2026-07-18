---
id: resilient-compact-temporal-bundles-project-manifests
kind: story
stage: done
tags: [agent-ux, visual]
parent: resilient-compact-temporal-bundles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Project compact manifests

## Checkpoint

Default bundle responses carry compact artifact handles and canonical manifest resource links; generic artifact results and complete retained provenance remain intact.

## Acceptance evidence

- Structured bundle size is bounded without repeated frame-ID arrays.
- Each manifest resource returns exact full provenance under validated evidence scope.

## Implementation notes

- Execution capability: frontier implementation; this changes the stable MCP presentation/resource boundary while preserving the generic artifact contract.
- Review weight: standard (project default; child checkpoint closes directly without independent review).
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/resources.rs`, `crates/krometrail-mcp/src/server.rs`.
- Tests added/removed: expanded the JSON-RPC resource qualification to assert compact bundle fields, absence of inline manifests/source-frame arrays, exact full-manifest JSON text, manifest template/link metadata, generic full-manifest preservation, and cross-target scope rejection; no tests removed.
- Simplification: one validated retained-artifact read supplies both image bytes and canonical manifest JSON; bundle compaction is an MCP-only projection over the existing typed result.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-mcp --all-targets --locked` — passed, 35 tests.
- `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings` — passed.
- `cargo fmt --all -- --check` — passed at the focused boundary.
- `git diff --check` — passed.
