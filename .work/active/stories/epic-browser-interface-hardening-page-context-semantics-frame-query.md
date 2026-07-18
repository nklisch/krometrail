---
id: epic-browser-interface-hardening-page-context-semantics-frame-query
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-browser-interface-hardening-page-context-semantics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Fingerprint-aligned frame semantics

Use one resolved frame document for AX and DOM semantic capture, return visible same-origin nested-frame matches, and reject document drift as stale evidence.

## Implementation notes

- Execution capability: inline Rust implementation; deterministic CDP fixtures cover the resolved-document handoff and final drift fence.
- Review weight: standard (default).
- Files changed: `crates/krometrail-cdp/src/control/snapshot.rs`.
- Tests added/removed: multi-document AX/DOM fixture proves a nested heading is resolved from the child document; navigation during capture returns `stale_reference`.
- Simplification: replaced tuple-shaped frame state and repeated ad-hoc construction with `ResolvedFrameDocument`.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-cdp --lib --locked snapshot::tests -- --nocapture`
- `cargo check -p krometrail-core -p krometrail-cdp --all-targets --locked`
