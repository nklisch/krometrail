---
id: epic-agent-surface-simplification-bounded-temporal-bundles-response
kind: story
stage: done
tags: [agent-ux, visual]
parent: epic-agent-surface-simplification-bounded-temporal-bundles
depends_on: [epic-agent-surface-simplification-bounded-temporal-bundles-anchor-scope]
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Project bounded temporal evidence progressively

Map generated bundle truth through concise/expanded/full detail, defer artifact byte reads until inline pixels are explicitly requested, update skill/foundation guidance, and regenerate public docs. Acceptance evidence proves bounded concise primary resources and exact counts, complete expanded/full outcomes for the selected scope, invariant warnings/gaps/range identity, and no default read-then-discard I/O.

## Implementation notes

- Execution capability: raised — one canonical temporal result is projected across three public detail levels and canonical MCP resources.
- Review weight: standard (autopilot caller); child story checkpoint, feature review owns the integrated pass.
- Files changed: MCP response/server/schema tests; `docs/VISUAL-EVIDENCE.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/EVALUATION.md`; Krometrail skill and temporal evidence reference.
- Tests added/removed: added generated anchor/all schema assertions and an end-to-end bundle test covering zero default artifact reads, one concise primary resource, exact outcome/resource counts, complete expanded outcomes/resources, and opt-in inline bytes.
- Simplification: replaced the shared compact/full bundle path with the common concise/expanded/full progression, removed the stale policy-version projection, and stopped publishing every retained resource in concise mode.
- Discrepancies from design: generated `docs/public/llms-full.txt` remained byte-identical because it composes public guide pages rather than the changed foundation/skill sources; `bun run docs:build` still verified the complete documentation build.
- Adjacent issues parked: none.

## Verification

- `cargo check --workspace --all-targets --locked`
- `cargo test -p krometrail-mcp generated_temporal_request_schemas_are_object_roots --locked`
- `cargo test -p krometrail-mcp successful_temporal_bundle_exposes_canonical_artifact_resource_end_to_end --locked`
- `bun run docs:build`
