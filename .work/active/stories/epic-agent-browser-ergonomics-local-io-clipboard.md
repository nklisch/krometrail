---
id: epic-agent-browser-ergonomics-local-io-clipboard
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

# Explicit managed-page clipboard

## Checkpoint

Implement Unit 1 of the parent design: bounded explicit read/write operations that preserve browser focus and permission authority, reject attachment, and redact content outside the explicit request/result.

## Acceptance evidence

The core, scripted-CDP, real-browser, and privacy tests named in Unit 1 pass without any clipboard permission mutation command.

## Implementation notes

- Added explicit, non-batchable managed-session `read_clipboard` and `write_clipboard` contracts with a 64 KiB UTF-8 bound and content-redacted `Debug` implementations.
- The CDP adapter uses fixed `Runtime.callFunctionOn` functions and passes write text as a protocol argument. It requires a visible, focused, secure page and leaves Chrome permission policy authoritative; it never activates a target or mutates browser permissions.
- Writes return ordinary post-operation evidence and persist an interaction anchor whose only clipboard-specific value is `utf8_bytes`. Reads return content only in the explicit tool result.
- Focused verification: `cargo test -p krometrail-core browser::`; `cargo check -p krometrail-core -p krometrail-cdp -p krometrail-mcp --all-targets`.
- Real-browser clipboard qualification remains part of the integrated feature verification because host clipboard policy is platform-owned and cannot be made hermetic in this story commit.
