---
id: epic-temporal-debugging-workflow-resolved-temporal-queries
kind: feature
stage: drafting
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Resolved Temporal Queries

## Brief

Deliver the application-facing temporal query boundary that turns explicit ranges, interaction-relative windows, recent interactions, markers, navigations, and source-frame anchors into one exact retained interval. Every request resolves once through the existing `TemporalRangeResolver` and returns the existing `ResolvedRange`, including requested and retained bounds, ordered source-frame identities, related timeline identities, declared gaps, and retention warnings.

Make interaction-relative querying operational rather than nominal by durably projecting the existing browser-operation interaction anchors and required navigation or marker timeline points into the store surfaces already consumed by range resolution. The browser executor remains the authority for action timing and identity; this feature persists and reads those same contracts rather than inferring timings from MCP responses or inventing a second interaction model.

This feature owns query validation, anchor resolution, and durable anchor availability. It does not decode frames, generate artifacts, correlate browser events, expose MCP routes, replay actions, or compare sessions.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: foundation capability — artifact generation and context evidence consume its once-resolved ranges

## Simplification opportunity

- Replace the current deliberate `InteractionAnchorSource` absence with one durable projection of the existing interaction contract, and keep all anchor forms behind the existing resolver. Do not add per-tool range parsing, a second `ResolvedRange`, or an in-memory production anchor cache.

## Foundation references

- `docs/SPEC.md` — Action Timeline, Temporal Ranges, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Interaction Execution, Temporal Range Resolution, and Recording Store
- `docs/VISUAL-EVIDENCE.md` — Progressive Detail and Markers
