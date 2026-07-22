---
id: feature-exact-query-native-control-miss
kind: feature
stage: drafting
tags: [agent-ux, browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Exact semantic queries can miss visible native controls

## Brief

GitHub issue #14 finding #4: a visible, enabled native button with rendered
text, `aria-expanded`, and `aria-controls` was absent from exact role/name and
exact-text `query_page` results, though the query reported a relaxed candidate.
CSS-selector activation of the same control worked and toggled `aria-expanded`,
so the control was real, actionable, and semantically labeled.

- Coordinate interaction: `9a443e44-e362-43ad-9f3c-6a2066cc0ba9`.
- CSS-selector interaction: `dc1c03c2-20fc-4e88-a9ef-a885e1f90dac`.

The relaxed-candidate surface behaved as designed; the defect is that exact
matching skipped a control it should have matched. Root-cause the accessible-
name computation / normalization path used by exact role/name and exact-text
matching for native controls (whitespace or nested-content normalization,
name-from-content rules, or state-bearing attributes affecting the computed
name are likely suspects), then fix exact matching to find it. Reproduce with a
deterministic fixture control mirroring the reported shape before fixing.

## Simplification opportunity

None identified; this is a bounded matching-fidelity fix inside the existing
query contract.

## References

- GitHub issue #14, finding 4 (macOS, authenticated local React app).
