---
id: gate-docs-reanchor-injected-ports-pattern
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

# Re-anchor injected-core-port examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/injected-core-ports.md:12`

## Contradiction
`RuntimeDependencies` and production connector composition moved to approximately `src/app.rs:46` and `:238`.

## Required edit
Re-anchor both examples to the live dependency structure and composition root.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored every example in `.agents/skills/patterns/injected-core-ports.md`, including the live dependency structure and production composition root; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
