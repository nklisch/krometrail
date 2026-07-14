---
id: epic-temporal-debugging-workflow-temporal-debug-bundle
kind: feature
stage: drafting
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-resolved-temporal-queries
  - epic-temporal-debugging-workflow-artifact-generation-and-cache
  - epic-temporal-debugging-workflow-capture-and-browser-event-context
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal Debug Bundle

## Brief

Deliver the primary single-range investigation capability. Given one already validated temporal query, compose a compact bundle containing the exact `ResolvedRange`, a concise non-diagnostic header, before/during/after orientation, change-aware storyboard, temporal difference map, source-frame and artifact references, complete provenance, and explicit capture-quality, gap, and retention warnings.

Combine visual measurements with timeline context deterministically: preserve interaction and navigation markers and select a bounded set of errors, failed requests, navigation, and browser events nearest the bundle's major visual-change moments. The bundle reports measurements and correlation distance as evidence, never causality or automatic diagnosis, and keeps full event sets and source images behind progressive references.

This feature owns bundle composition and default evidence policy. It does not duplicate artifact algorithms, include motion history in the default bundle before evaluation earns it, compare interactions or sessions, replay actions, track logical elements, or decide MCP wire/resource presentation.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: primary investigation capability — joins resolved queries, generated artifacts, and recorded context; consumed by the MCP surface

## Simplification opportunity

- Compose the existing artifact results and one context query into a single bundle service. Do not add a second storyboard selector, difference metric, gap model, event store, provenance schema, or per-artifact bundle response family.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, and Visual Evidence
- `docs/SPEC.md` — Temporal Queries and Artifact Provenance
- `docs/ARCHITECTURE.md` — Artifact Generation and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — Before/During/After Composite, Temporal Debug Bundle, and Progressive Detail
