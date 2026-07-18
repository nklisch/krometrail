---
id: feature-headless-managed-browser
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Launch managed browsers without taking desktop focus

Add an explicit managed-browser launch mode that runs Chrome headlessly so an agent can navigate,
interact, capture screenshots, and retain temporal evidence without creating or foregrounding a
desktop window. Preserve the stable visible-browser default and attached-browser behavior; callers
choose headless operation when unattended control matters more than watching the browser.

## Strategic decisions

- **Compatibility default**: keep visible managed launch as the default — existing 1.x callers may
  rely on watching or manually interacting with the launched browser.
- **Scope boundary**: apply headless mode only to managed launches — attachment cannot change how an
  externally owned browser process was started.
- **Agent guidance**: teach the shipped skill to request headless mode for unattended operation and
  visible mode when the user wants to observe or share control.

## Simplification opportunities

- Extend the existing generated `LaunchBrowser` contract and launcher argument assembly; do not add
  a second launcher path or process-wide configuration authority.
- Reuse the existing headless Chrome qualification support rather than inventing a browser backend.
