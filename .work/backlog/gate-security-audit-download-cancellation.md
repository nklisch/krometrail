---
id: gate-security-audit-download-cancellation
kind: story
tags: [security]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: security
created: 2026-07-18
updated: 2026-07-18
---

# Persist privacy-safe download-cancellation evidence

## Severity
Low

## Domain
Error Handling & Logging

## Location
`crates/krometrail-cdp/src/session/evidence.rs:125`

## Evidence

```rust
| BrowserOperationResult::CancelDownload(_)
```

`cancel_download` is state-changing and requires an evidence sink, but successful cancellation is projected to no interaction evidence.

## Remediation direction

Persist a privacy-safe browser-scoped cancellation record containing stable operation metadata and opaque download identifier/state only, and surface persistence uncertainty consistently.
