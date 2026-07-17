---
id: epic-agent-browser-reliability-capture-outcomes-compositor-readiness
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-capture-outcomes
depends_on: [epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Wait for bounded compositor readiness before automatic screenshots

Before automatically capturing a post-action viewport, await two renderer animation-frame callbacks under a 250 ms cancellation-aware cap. Proceed with a sanitized diagnostic when the signal is unavailable, and leave explicit screenshots/standalone live observations immediate.

## Acceptance evidence

- Scripted transport tests prove readiness evaluation precedes automatic `Page.captureScreenshot` and is absent from explicit screenshot/live-observation calls.
- Timeout tests prove hidden or unresponsive targets proceed within the cap without rewriting the action outcome.
- Opt-in Chrome qualification against the frame-delayed observation fixture returns the complete final viewport state.

## Ordering

Depends on `epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence` because compositor fallback must use its non-fatal post-dispatch evidence semantics.
