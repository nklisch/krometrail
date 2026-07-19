---
id: runtime-observation-hardening-snapshot-projections
kind: story
stage: done
tags: [agent-ux, browser]
parent: runtime-observation-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Prioritize interactions and add an interaction-only snapshot projection

Replace preorder-dominated compact selection with deterministic actionable ranking and add the snapshot-only `interaction_only` response detail. Both projections derive from the complete canonical snapshot, preserve exact reference authority, admit complete ancestor closures within existing budgets, and report presentation omissions exactly.

## Implementation notes

- Execution capability: inline implementation; the MCP response enum, selector, boundary tests, plugin guidance, and standing docs are one cohesive presentation contract.
- Review weight: standard (project default).
- Root cause: the prior two-pass compactor marked every actionable ancestor as priority but admitted those nodes in raw preorder, so early links could exhaust the 48-node budget before a later editable control. Snapshot and page-state also shared one enum, preventing a snapshot-only interaction projection.
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `plugin/skills/krometrail/SKILL.md`, and generated `docs/public/llms-full.txt`.
- Implementation: `SnapshotResponseDetail` adds only `interaction_only`, while page-state retains `StructuredResponseDetail`. One `project_snapshot(PageSnapshot, SnapshotProjection)` ranks focused, editable, other non-link, then link actions with preorder ties; atomically admits each complete missing ancestor closure; emits original preorder; and computes exact presentation omissions. Compact preserves under-budget snapshots byte-for-byte and fills remaining over-budget capacity with preorder context; interaction-only never performs context fill.
- Tests added/changed: schema exposes `interaction_only` only for snapshots and rejects it for page-state; early links cannot displace later focused/editable/non-link actions; live and root interaction-only projections preserve context, generation, exact references, ancestry, and omission counts; an over-byte-budget ancestor closure is rejected atomically; existing full, legacy, compact, omit, node/byte-budget, and small-byte-equivalence coverage remains green.
- Verification: the ranking regression and schema test failed before implementation; `cargo test -p krometrail-mcp --lib --locked` passes 66 tests; `bun run docs:build` regenerated the public aggregate and completed VitePress successfully.
- Simplification: replaced the priority-set plus preorder admission path with one shared ranked closure selector; no second snapshot model or acquisition path was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.
