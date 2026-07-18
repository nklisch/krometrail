---
id: gate-docs-reanchor-single-writer-pattern
kind: story
stage: implementing
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
