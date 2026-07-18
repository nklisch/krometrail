---
id: gate-docs-reanchor-injected-ports-pattern
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

# Re-anchor injected-core-port examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/injected-core-ports.md:12`

## Contradiction
`RuntimeDependencies` and production connector composition moved to approximately `src/app.rs:46` and `:238`.

## Required edit
Re-anchor both examples to the live dependency structure and composition root.
