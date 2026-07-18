---
id: gate-docs-reanchor-bounded-loss-pattern
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

# Re-anchor bounded-loss pattern examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/bounded-loss-accounting.md:11`

## Contradiction
The cited capture line is a closing expression; full/closed queue loss branches moved near lines 573 and 576.

## Required edit
Re-anchor the example to the current full/closed typed gap branches.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored all three examples in `.agents/skills/patterns/bounded-loss-accounting.md` to the live queue-loss branches; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
