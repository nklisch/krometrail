---
id: epic-agent-browser-ergonomics-browser-contexts-profile-inventory
kind: story
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-browser-contexts
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Privacy-bounded managed profile inventory

Implement Unit 1 of the parent design: connector-owned reusable-profile discovery and the lifecycle tool that exposes identity plus in-use state without paths or browser data.

Acceptance evidence is the core/launcher/schema test slice listed in Unit 1. This checkpoint is independent of the other browser-context units.

## Implementation notes

- Execution capability: direct inline implementation within the parent feature bundle.
- Added connector-owned, session-independent reusable-profile discovery. It enumerates only validated direct directories, excludes symlinks and temporary profiles, sorts identities, and projects only `identity` plus lock-derived `in_use`.
- Published `list_managed_profiles` as a read-only lifecycle tool callable without an active browser. Root access failures map to a source-safe page-observation failure.
- Verification: core/CDP workspace check, launcher inventory unit tests, MCP route/schema registry tests.
