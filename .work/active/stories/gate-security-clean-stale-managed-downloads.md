---
id: gate-security-clean-stale-managed-downloads
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

## Acceptance

- Activation recognizes cleanup candidates only when their direct-child name is the exact canonical non-nil UUID spelling used by session IDs.
- An owner-held marker lock distinguishes active directories; current and locked active sessions are never removed.
- Canonical-name symlinks, non-directories, malformed markers, enumeration failures, and removal failures fail activation with privacy-safe `persistence_failed` recovery and no path disclosure.
- Noncanonical files/directories and unrelated siblings are ignored, never followed or removed.
- The active marker closes before normal cleanup so shutdown remains cross-platform and scoped to its one session root.

## Tests

Use temp roots to cover stale removal, current/active preservation, unrelated sibling preservation, symlink/non-directory rejection, and cleanup error reporting; run focused downloads and CDP clippy.

## Implementation and review

Managed-download activation now coordinates scavenging with a private root lock and holds one owner lock for each live session directory. Only exact canonical non-nil UUID direct children are candidates; current/locked roots survive, unrelated names are ignored, and canonical-name symlinks, non-directories, malformed markers, or cleanup failures stop local-I/O activation with privacy-safe `persistence_failed` recovery. The lease closes before shutdown/drop removes its root. Focused download tests pass 14/14 and CDP all-target clippy passes at the workspace MSRV. Bounded inline review checked Unix no-follow locking, Windows exclusive-share ownership, current/active preservation, and failure-path lock release. Verdict: pass.

## Promotion

Promoted from the low-severity backlog because the operator requested every release-gate finding be resolved before shipment.
