---
id: gate-docs-reanchor-single-writer-pattern
kind: story
stage: done
tags: [documentation]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: docs
created: 2026-07-18
updated: 2026-07-18
---

# Re-anchor single-writer reducer examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/single-writer-effect-reducer.md:12`

## Contradiction
Lifecycle input, operation reduction, and runtime effect reduction moved near `model.rs:272`, `operations.rs:928`, and `runtime.rs:233`.

## Required edit
Re-anchor all three examples to the live reducer authorities.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored lifecycle input, operation commit, and runtime reduction examples in `.agents/skills/patterns/single-writer-effect-reducer.md`; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.

## Inline review

- Verdict: pass. Input, commit, and runtime reduction citations land on the live single-writer authorities.
