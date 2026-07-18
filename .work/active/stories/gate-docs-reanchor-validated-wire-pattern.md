---
id: gate-docs-reanchor-validated-wire-pattern
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

# Re-anchor validated-wire contract example

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/validated-wire-contracts.md:12`

## Contradiction
The cited line is now a schema assertion; `AttachBrowserWire` and delegated schema moved near `browser.rs:290` and `:297`.

## Required edit
Re-anchor the example to the current wire type/schema boundary.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored `AttachBrowserWire` in `.agents/skills/patterns/validated-wire-contracts.md` to the current wire/schema boundary; the remaining examples still resolve and guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
