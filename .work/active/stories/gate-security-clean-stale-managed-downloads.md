---
id: gate-security-clean-stale-managed-downloads
kind: story
stage: implementing
tags: [security]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: security
created: 2026-07-18
updated: 2026-07-18
---

# Clean verified stale managed-download session directories

## Severity
Low

## Domain
Data Protection

## Location
`crates/krometrail-cdp/src/session/downloads.rs:759`

## Evidence

```rust
let root = base.join(session_id.to_string());
std::fs::create_dir(&root).map_err(|_| {
```

Normal shutdown removes only the active session directory; a crash can leave sensitive completed or partial sibling directories outside a retention lifecycle.

## Remediation direction

During managed-download activation, safely enumerate and remove verified stale session directories under the canonical managed root with symlink rejection and explicit cleanup-failure diagnostics.

## Promotion

Promoted from the low-severity backlog because the operator requested every release-gate finding be resolved before shipment.
