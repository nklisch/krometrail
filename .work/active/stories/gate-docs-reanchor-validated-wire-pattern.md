---
id: gate-docs-reanchor-validated-wire-pattern
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

# Re-anchor validated-wire contract example

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/validated-wire-contracts.md:12`

## Contradiction
The cited line is now a schema assertion; `AttachBrowserWire` and delegated schema moved near `browser.rs:290` and `:297`.

## Required edit
Re-anchor the example to the current wire type/schema boundary.
