---
id: gate-docs-reanchor-registry-pattern
kind: story
stage: review
tags: [documentation]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: docs
created: 2026-07-18
updated: 2026-07-18
---

# Re-anchor registry-declared surface examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/registry-declared-surfaces.md:12`

## Contradiction
Browser operation declaration and progressive MCP iteration moved near `operation.rs:132` and `registry.rs:210`.

## Required edit
Re-anchor both examples to the live registry loops.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored every example in `.agents/skills/patterns/registry-declared-surfaces.md`, including the browser-operation declaration and progressive MCP loop; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
