---
id: feature-semantic-wait
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Semantic wait

## Brief

GitHub issue #14 finding #10: exact-text waits are brittle when a control
includes additional copy beyond the identifying text, and there is no
first-class role/name wait aligned with how `query_page` targets controls. A
caller who interacts semantically (role + accessible name) must fall back to
text matching or locator waits to await the resulting state.

Extend `wait` with a semantic condition that reuses the existing `query_page`
query shapes (role/name, label, text, test id — same exact/contains modes and
normalization) so waiting and targeting speak one language. One registry of
query shapes drives both surfaces (registry-declared-surfaces); do not fork a
second matching implementation for waits.

## Simplification opportunity

If the semantic condition subsumes common uses of the exact-text wait, design
should check whether text-wait guidance (and any awkward matching options that
existed only to approximate semantic waits) can be simplified.

## References

- GitHub issue #14, finding 10 (wait ergonomics).
