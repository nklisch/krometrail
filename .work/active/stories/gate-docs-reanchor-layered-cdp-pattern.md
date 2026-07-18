---
id: gate-docs-reanchor-layered-cdp-pattern
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

# Re-anchor layered CDP qualification example

## Drift category
pattern-skill-staleness

## Location
`.agents/skills/patterns/layered-cdp-qualification.md:14`

## Contradiction
The cited line is scripted setup; the opt-in real-Chrome reconnect qualification begins near `session_supervision.rs:492`.

## Required edit
Re-anchor the real-browser example to its current test.
