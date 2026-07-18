---
id: epic-agent-browser-ergonomics-local-io-resource-surface
kind: story
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-local-io
depends_on: [epic-agent-browser-ergonomics-local-io-download-authority]
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Active-session download resources

## Checkpoint

Implement Unit 3 of the parent design after the download authority exists: strict canonical resource reads through `BrowserSessionOwner` plus installed skill lifetime/privacy guidance.

## Acceptance evidence

Canonical URI, byte-exact active-session read, post-stop invalidation, schema/resource registry, and plugin static tests named in Unit 3 pass.

## Implementation notes

- Added the strict canonical `krometrail://local/{session}/downloads/{download}` resource template and parser. Percent escapes, query, fragment, backslash, extra segments, noncanonical UUID spelling, and alternate schemes fail before session lookup.
- Resource reads route only through `BrowserSessionOwner` to the active session authority, return one bounded `application/octet-stream` blob with exact bytes, and map stopped/replaced/in-progress/mismatched resources to resource-not-found without exposing a path.
- The installed Krometrail skill now explains explicit permission/focus-preserving clipboard calls, cursor-before-action download waiting, local-only resource handling, and stop/session-loss invalidation. The evidence reference distinguishes active-session downloads from retained temporal evidence.
- Verification: MCP resource tests 8/8, owner lifetime tests 3/3, the in-memory JSON-RPC test covers template discovery, byte-exact base64 resource read, and post-stop invalidation, and the full MCP suite passes.
