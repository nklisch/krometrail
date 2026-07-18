---
id: epic-agent-browser-ergonomics-browser-contexts-page-relationships
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-browser-contexts
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Popup relationships and page waits

Implement Unit 2 of the parent design: monotonic page-context inventory, opener projection, and race-safe page waits without changing `list_pages`.

Acceptance evidence is the reducer/session/registry test slice listed in Unit 2. This checkpoint is independent of profile and asset discovery.

## Implementation notes

- Execution capability: direct inline implementation within the parent feature bundle.
- Target supervision now assigns monotonically increasing page sequences, retains opener relationships as resolved Krometrail target identities, and never rebinds a popup when a raw opener key is reused.
- Added `list_page_contexts` and bounded `wait_for_page`. The wait checks retained state first, polls the browser target inventory without activation/focus, reconciles new targets through the single-writer reducer, and enforces cursor, opener, cancellation, and timeout fences.
- Verification: all eight target reducer integration tests, bounded request serde tests, workspace all-target check, MCP route/schema registry tests.
