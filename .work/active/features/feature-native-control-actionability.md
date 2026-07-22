---
id: feature-native-control-actionability
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Native control actionability

## Brief

Two GitHub issue #14 findings show ordinary semantic interactions failing
against common native controls, forcing CSS fallbacks or low-level choreography:

- **Upload affordance does not resolve to its native input (finding #2).**
  `upload_files` against the visible accessibility reference failed with
  `reference_not_actionable`, while targeting the hidden native file input by
  CSS selector succeeded. The semantic upload affordance should resolve to its
  associated native input (label association, wrapping, or aria linkage), or
  the failure should identify that input as the required target. Correlation:
  `9fbbd5bf-71cd-41b6-bbbf-0d90dd302079`.
- **Native date inputs cannot be filled through ordinary interactions
  (finding #3).** The date field was represented semantically, including
  month/day/year spinbuttons, but `fill` against both the native input selector
  and the structured date reference failed because the backing node was invalid
  for the requested interaction. A normal correction workflow required
  low-level key choreography or DOM evaluation. `fill` should support native
  date/time inputs with a validated value and proper events, or fail with an
  explicit guided path.

Both fit the ergonomic-input-canonicalization pattern: materialize the semantic
affordance into the explicit native authority the browser actually requires,
keeping the convenience form as provenance.

## Simplification opportunity

None identified beyond folding the resolution into the existing actionability
checks rather than adding a parallel pre-flight surface.

## References

- GitHub issue #14, findings 2 and 3 (macOS, authenticated local React app).
