---
id: agent-visual-response-surface-followup-contracts
kind: story
stage: done
tags: [agent-ux, browser, testing]
parent: agent-visual-response-surface
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Publish discoverable follow-up schemas and chronological event detail

Replace constraint-only opaque range-handle unions with complete concrete branches, preserve exact-one validation, and keep bounded chronological event rows in the default event-detail response.

## Implementation notes

- Execution capability: GPT-5.6, high reasoning; the change crosses generated schemas and canonical MCP response projection but remains one cohesive boundary.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/server.rs`.
- Tests added/updated: complete concrete range/handle branch assertions, concise chronological event-row preservation, and end-to-end range-handle event response coverage.
- Simplification: constraint-only `required`/`not` branches were replaced directly by two self-contained accepted object shapes.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-mcp --lib schema::tests --locked`; `cargo test -p krometrail-mcp --lib response::tests::temporal_detail_defaults_to_concise_and_full_preserves_rows --locked`.
- Workflow deviation: `.work/bin/work-view` is an x86-64 Linux binary unavailable on this macOS host; dependency readiness was verified from direct item frontmatter.
