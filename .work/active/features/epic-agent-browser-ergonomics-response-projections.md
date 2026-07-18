---
id: epic-agent-browser-ergonomics-response-projections
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Agent-sized response projections

## Brief

Add one validated MCP presentation preference for browser operations and temporal entry points, plus a concise `browser_status` detail mode. Callers can independently omit inline images or select compact structured observations where the existing projector has a truthful compact representation. Omitted preferences retain full stable 1.x output, and underlying action observation, retention, warnings, diagnostics, and canonical resources remain unchanged.

This feature does not introduce persistence for live screenshots or skip required post-action observation. It teaches the Krometrail skill to request the cheapest sufficient projection and to drill into explicit snapshot, screenshot, status, or resource tools only when needed.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent MCP presentation contract used by routine agent workflows

## Simplification opportunity

Extend the shared response projector and lifecycle argument schema rather than adding compact variants per tool or duplicating `BrowserStatus` in the domain.

## Foundation references

- `docs/SPEC.md` — Current-State Observation
- `docs/ARCHITECTURE.md` — MCP Boundary
