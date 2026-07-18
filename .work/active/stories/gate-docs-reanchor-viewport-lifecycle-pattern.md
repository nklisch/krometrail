---
id: gate-docs-reanchor-viewport-lifecycle-pattern
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

# Re-anchor viewport lifecycle pattern examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/lifecycle-complete-browser-overrides.md:11`

## Contradiction
1.1 moved apply, rollback, navigation replay, and complete command authorities; the current citations show unrelated or partial code.

## Required edit
Re-anchor apply/commit/rollback/replay/clear examples after release code stabilizes.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored apply/commit, rollback, navigation replay, reconnect replay, and complete apply/clear authorities in `.agents/skills/patterns/lifecycle-complete-browser-overrides.md`; guidance is unchanged.
- Verification: exact source branches/functions inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
