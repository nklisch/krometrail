---
id: epic-agent-browser-ergonomics-browser-contexts
kind: feature
stage: drafting
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics
depends_on: [epic-agent-browser-ergonomics-semantic-targeting]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Browser contexts and assets

## Brief

Expose the browser contexts agents need to navigate deliberately: list reusable named managed profiles without paths, preserve popup opener relationships and wait for newly created pages, inventory frames, scope semantic inspection and interaction to qualified same-origin frames, and list privacy-bounded current-page asset metadata. Update the agent skill to explain reusable named profiles and when to choose them instead of temporary profiles or attachment.

The work excludes raw resource bodies, unrestricted DevTools commands, cross-origin frame DOM access, and unqualified OOPIF interaction. Unsupported browser/context variants fail explicitly rather than falling back to main-document coordinates.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: consumer of main-document semantic targeting and producer of explicit browser-context scope

## Simplification opportunity

Reuse target supervision for popup/frame identity, sanitized network/resource metadata for assets, and the existing managed-profile launcher. Do not create a second browser automation layer.

## Foundation references

- `docs/SPEC.md` — Browser-Control Surface and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Target Lifecycle, MCP Boundary, and Observability
