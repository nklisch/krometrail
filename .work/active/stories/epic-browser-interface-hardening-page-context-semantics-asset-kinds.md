---
id: epic-browser-interface-hardening-page-context-semantics-asset-kinds
kind: story
stage: done
tags: [browser]
parent: epic-browser-interface-hardening-page-context-semantics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Reconcile asset kinds

Classify unambiguous URL extensions before falling back to Resource Timing initiator type, including JavaScript modules and web fonts.

## Implementation notes

- Execution capability: inline Rust implementation; the sanitized URL preserves the allowlisted extension needed for deterministic classification.
- Review weight: standard (default).
- Files changed: `crates/krometrail-cdp/src/control/contexts.rs`.
- Tests added/removed: extension override coverage for module scripts and web fonts, plus CSS/image/media and extensionless fetch/XHR fallbacks.
- Simplification: centralized resource-kind reconciliation in one helper.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-cdp --lib --locked contexts::tests -- --nocapture`
