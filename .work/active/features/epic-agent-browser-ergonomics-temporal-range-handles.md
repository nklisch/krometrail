---
id: epic-agent-browser-ergonomics-temporal-range-handles
kind: feature
stage: drafting
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Temporal resolved-range handles

## Brief

Return an opaque handle alongside resolved temporal ranges and accept that handle anywhere an agent currently has to repeat the full range: artifacts, region filmstrips, source-frame reads, browser events, pin state, and optional video. The process-local authority maps the handle back to the exact validated `ResolvedRange` before existing services run and revalidates retained availability on use.

Handles are immutable conveniences, not persisted evidence identities. They survive browser stop while retained data remains, fail after MCP restart or session deletion, and never replace the full-range contract or canonical provenance.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent temporal-agent ergonomics contract

## Simplification opportunity

Resolve handle-or-range once at the application boundary and keep every store, artifact, browser-event, retention, and video port expressed in exact `ResolvedRange` values.

## Foundation references

- `docs/SPEC.md` — Temporal Ranges and Temporal Queries
- `docs/ARCHITECTURE.md` — Temporal Range Resolution and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — provenance and authoritative source frames
