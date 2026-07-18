---
id: gate-docs-reanchor-exact-release-pattern
kind: story
stage: implementing
tags: [documentation]
parent: null
depends_on: []
release_binding: null
gate_origin: docs
created: 2026-07-18
updated: 2026-07-18
---

# Re-anchor exact-release activation examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/exact-release-managed-activation.md:13`

## Contradiction
Three annotations now land on unrelated/blank lines; current constructs are near `plugin/bin/krometrail:13`, `scripts/bump-version.ts:134`, and `tests/plugin-static.sh:46`/`:89`.

## Required edit
Replace the stale anchors while retaining the current pattern guidance.
