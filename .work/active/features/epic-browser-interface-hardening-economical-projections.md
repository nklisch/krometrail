---
id: epic-browser-interface-hardening-economical-projections
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Economical Agent Projections

## Brief

Make automatic live snapshots and default temporal bundles reliably small enough for routine agent use while retaining explicit full/canonical drill-down. The current live limits still allow a 32 KiB/96-node payload that dominates mutations, and temporal compaction is incorrectly coupled to snapshot/page-state preference fields so a request that omits those unrelated parts can receive the full temporal bundle.

Preserve acquisition and canonical evidence. Put all change in the MCP presentation layer, publish deterministic omission/count summaries, and test serialized size rather than assuming node or row counts imply ergonomic output.

## Source findings

- `idea-bound-compact-snapshot`
- `idea-compact-temporal-bundle`

## UI alignment

No UI surface; this is an MCP response-projection feature.
