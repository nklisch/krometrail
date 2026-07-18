---
id: gate-security-audit-download-cancellation
kind: story
stage: done
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

## Acceptance

- A successful `cancel_download` result carries a browser-operation anchor and persists one record through the existing evidence sink.
- Sanitized parameters contain exactly opaque `download_id` and terminal/current `state`; GUIDs, paths, names, URLs, and bytes are absent.
- Evidence persistence failure returns the existing non-retryable `persistence_failed` uncertainty after cancellation, without suggesting replay is safe.
- Cancellation transport behavior and public ID/state response remain otherwise unchanged.

## Tests

Add projection tests for exact sanitized keys and sentinel absence, then run focused evidence/download tests and CDP clippy.

## Implementation and review

Cancellation now returns a stable operation anchor partitioned by the current supervised page and persists through the existing interaction-evidence sink. The record contains exactly `download_id` and `state`; the authority's private GUID, filename, URL, path, resource URI, and bytes remain absent. Persistence uncertainty therefore follows the same non-replay-safe path as every other state-changing operation. Evidence tests pass 3/3, download tests 10/10, and CDP all-target clippy passes. Bounded inline review confirmed the selected page is only the existing target-scoped evidence partition and does not claim download-target attribution. Verdict: pass.

## Promotion

Promoted from the low-severity backlog because the operator requested every release-gate finding be resolved before shipment.
