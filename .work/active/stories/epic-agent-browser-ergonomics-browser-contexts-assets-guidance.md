---
id: epic-agent-browser-ergonomics-browser-contexts-assets-guidance
kind: story
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-browser-contexts
depends_on: []
release_binding: 1.1.0
research_refs: []
research_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Page assets and browser-context guidance

Implement Unit 4 of the parent design: bounded Resource Timing metadata plus installed skill coverage for profiles, popup cursors, qualified frames, and assets.

Acceptance evidence is the privacy/schema/plugin fixture slice listed in Unit 4. Asset bodies, raw URLs, and local paths remain out of scope.

## Implementation notes

- Execution capability: direct inline implementation within the parent feature bundle.
- Added an adapter-owned side-effect-free Resource Timing projection, finite/non-negative validation, deterministic start-time/sanitized-URL ordering, a 256-row bound, and explicit malformed/truncated omission accounting.
- Results expose only sanitized URL shape, initiator kind, duration, and browser-disclosed sizes; zero sizes remain present and no headers, bodies, cookies, raw URLs, or paths enter the result or diagnostics.
- Updated the installed skill and a dedicated browser-context reference with low-cost defaults, profile choice, popup cursor, frame failure, and asset privacy guidance.
- Verification: asset privacy decoder tests, MCP route/schema tests, workspace all-target check.
