---
id: epic-agent-browser-ergonomics-local-io-download-authority
kind: story
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-local-io
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bounded managed-download authority

## Checkpoint

Implement Unit 2 of the parent design: private session directory, browser-level lifecycle reducer, bounded completion/cancellation, privacy-safe events, and idempotent shutdown cleanup.

## Acceptance evidence

The reducer, filesystem-boundary, scripted-CDP, real-browser, and privacy tests named in Unit 2 pass; attached sessions touch neither permission nor filesystem authority.

## Implementation notes

- Added one serialized per-session authority for browser-level `downloadWillBegin`/`downloadProgress` events. Managed launch subscribes before enabling Chrome `allowAndName`; attached sessions create no directory and issue no download command.
- Files remain Chrome GUID-named under a private `0700` session directory. Completion publishes only after an exact-size, regular, non-symlink file canonicalizes below that root. A 32-entry/64 MiB bound cancels and removes rejected partials.
- `list_downloads`, cursor-safe `wait_for_download`, and idempotent `cancel_download` route through the browser operation registry. Stop and session-loss paths close admission, cancel active GUIDs, and remove only the active session directory; cleanup failure degrades the stop result.
- Focused verification: four reducer/filesystem tests cover exact reads and scoped cleanup, oversize cancellation, symlink rejection, and cursor-before-change waiting; `cargo check -p krometrail-cdp --all-targets` passes.
- Real-browser download qualification remains at the integrated feature boundary, where the installed Chrome build and browser-level event behavior are available.
