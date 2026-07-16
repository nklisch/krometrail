---
id: gate-security-redact-nested-browser-event-secrets
created: 2026-07-15
updated: 2026-07-15
tags: [security, browser, cleanup]
gate_origin: security
release_binding: null
---

Harden browser-event text redaction for minified or nested structured values such as `{"outer":{"token":"secret"}}`. The current bounded whitespace/token sanitizer can miss a sensitive key nested inside a compact JSON fragment. Add structure-aware regression coverage while retaining bounded useful console evidence; this is defense in depth and does not block the local-tool 1.0 release.
