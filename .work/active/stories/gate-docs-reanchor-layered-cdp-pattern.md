---
id: gate-docs-reanchor-layered-cdp-pattern
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

# Re-anchor layered CDP qualification example

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/layered-cdp-qualification.md:14`

## Contradiction
The cited line is scripted setup; the opt-in real-Chrome reconnect qualification begins near `session_supervision.rs:492`.

## Required edit
Re-anchor the real-browser example to its current test.

## Implementation notes

- Execution capability: direct-read inline prose; the change is a small factual anchor correction.
- Review weight: standard (project default), using bounded inline standalone-story review.
- Re-anchored the real-Chrome reconnect example in `.agents/skills/patterns/layered-cdp-qualification.md` to the current opt-in test; guidance is unchanged.
- Verification: exact source lines inspected, catalog-wide static anchor check passed, and `bun run docs:build` passed.
- Discrepancies and adjacent issues: none.
