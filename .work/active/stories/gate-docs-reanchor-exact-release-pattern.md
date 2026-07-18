---
id: gate-docs-reanchor-exact-release-pattern
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

# Re-anchor exact-release activation examples

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/exact-release-managed-activation.md:13`

## Contradiction
Three annotations now land on unrelated/blank lines; current constructs are near `plugin/bin/krometrail:13`, `scripts/bump-version.ts:134`, and `tests/plugin-static.sh:46`/`:89`.

## Required edit
Replace the stale anchors while retaining the current pattern guidance.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored launcher, release transaction, and static-contract examples in `.agents/skills/patterns/exact-release-managed-activation.md`; the installer anchor remained exact and guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
