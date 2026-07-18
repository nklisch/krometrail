---
id: epic-agent-browser-ergonomics
kind: epic
stage: drafting
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Agent browser ergonomics

## Brief

Make Krometrail's evidence-rich browser surface as economical and composable for ordinary agent work as contemporary Playwright-backed browser controls, without weakening its stable 1.x evidence, recovery, and provenance contracts. The work follows a live comparison against Codex's isolated in-app browser and Chrome extension: Krometrail led on temporal evidence, responsive-device precision, mutation verification, and structured recovery, but required larger payloads, more snapshot round-trips, verbose resolved-range copying, and more browser-lifecycle knowledge.

Deliver additive response projections and concise status, semantic locator queries with explicit frame scope, reusable temporal range handles, intention-revealing viewport presets and mismatch guidance, and a targeted parity layer for browser state agents routinely need to inspect or control. Reusable named managed profiles already exist; this epic improves their agent-facing discoverability instead of creating a second persistence system.

The resulting stable surface remains local-first and evidence-oriented. Full observations, exact resolved ranges, current custom viewport metrics, canonical resources, and existing request meanings remain available for 1.x clients. Compactness and convenience are new choices layered over those authorities.

## Strategic decisions

- **Compatibility**: make every 1.x contract extension additive; omitted new fields preserve current request and response meaning.
- **Product position**: optimize Krometrail for trustworthy diagnosis and temporal evidence while adding the smallest browser-control conveniences that remove repeated agent friction; do not recreate all of Playwright.
- **Response authority**: projections may omit inline presentation detail but never discard retained evidence or change the authoritative domain result.
- **Browser-state boundary**: add privacy-bounded metadata and explicit user-directed control for clipboard, downloads, frames, popups, and page assets; do not broaden into unrestricted DevTools or response-body capture.
- **Profile continuity**: retain named Krometrail-managed profiles as the supported continuity mechanism and teach agents when to select one; attachment to the user's default browser remains explicit.

## Simplification opportunity

Consolidate response selection in the existing MCP projector, locator resolution in the existing snapshot/reference registry, temporal follow-up inputs behind one range authority, and viewport intent in the existing target-scoped override lifecycle. Reuse target supervision and browser-event infrastructure for popup/download discovery. Avoid parallel schema registries, duplicated range persistence, a second profile manager, or tool-specific compacting logic.

## Anticipated child features

- Agent-sized response projections and concise status.
- Semantic locators and frame-scoped targeting.
- Reusable temporal range handles.
- Viewport intent presets and effective-layout guidance.
- Managed-state guidance and targeted browser parity for clipboard, downloads, popups, frames, and page assets.

## Release intent

Ship the completed epic as the next minor release after feature and aggregate review. Release binding remains late-bound until implementation and review are complete.
