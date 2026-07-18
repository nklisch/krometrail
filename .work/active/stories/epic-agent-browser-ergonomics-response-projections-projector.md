---
id: epic-agent-browser-ergonomics-response-projections-projector
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-response-projections
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Shared response projection contract and projector

## Checkpoint

Introduce the validated additive `response` preference, decorate generated route schemas, and apply full/compact/omit presentation through the one MCP response projector after authoritative operation or temporal acquisition. Preserve outcome, interaction, warnings, errors, resource identities, and screenshot availability metadata while bounding representative compact responses.

## Acceptance evidence

- Schema-wide tests prove omitted preferences preserve underlying required fields while selecting the economical projection, explicit legacy/full expansion remains available, and invalid nested preferences fail without echoing values.
- Response-layer tests prove mutation, batch, and temporal projection combinations preserve authoritative fields and suppress only selected presentation content.
- A deterministic large snapshot/bundle fixture demonstrates an economical projection with an explicit serialized-size bound.

## Ordering

This checkpoint establishes the vocabulary and transformation used by route, status, diagnostics, and skill integration.

## Implementation notes

- Execution capability: direct inline implementation; the response and schema modules were a cohesive two-file boundary with no unresolved integration unknowns.
- Review weight: standard (project default); review applies at the integrated feature boundary, not this child checkpoint.
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`.
- Tests added/removed: added projection decoding/privacy, full/compact/omit snapshot, inline-image omission, temporal payload bound, and additive schema-decoration tests; removed none.
- Simplification: renamed the existing automatic snapshot compactor as the shared compact snapshot path and kept one post-acquisition projector instead of adding compact tool variants.
- Discrepancies from design: existing generated operation roots omit `additionalProperties` and therefore advertise open roots; schema decoration preserves that stable 1.x shape while adding a closed nested `response` object instead of silently closing legacy inputs.
- Adjacent issues parked: none.
- Direction update: the user explicitly chose economical server defaults after the first checkpoint landed; route integration changes omitted response preferences to compact/no-inline while retaining explicit `legacy`, `full`, and `inline` expansion.

## Verification

- `cargo test -p krometrail-mcp response::tests --locked`
- `cargo test -p krometrail-mcp schema::tests --locked`
- `cargo check -p krometrail-mcp --all-targets --locked`
