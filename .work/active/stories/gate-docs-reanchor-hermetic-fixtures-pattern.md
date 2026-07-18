---
id: gate-docs-reanchor-hermetic-fixtures-pattern
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

# Re-anchor hermetic release-fixture examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/hermetic-release-boundary-fixtures.md:13`

## Contradiction
Example annotations moved to approximately lines 17, 45, 55, 21, and 338 in their respective fixture files.

## Required edit
Re-anchor every example to the current fixture/function boundary.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored every fixture/function example in `.agents/skills/patterns/hermetic-release-boundary-fixtures.md`; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
