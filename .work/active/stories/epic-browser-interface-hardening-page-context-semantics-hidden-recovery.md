---
id: epic-browser-interface-hardening-page-context-semantics-hidden-recovery
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

# Make hidden-target recovery truthful

Return focus-policy-aware target-hidden messages and recovery that only names operations and request options agents can actually use.

## Implementation notes

- Execution capability: inline Rust implementation; focus policy is already the interaction-preparation authority.
- Review weight: standard (default).
- Files changed: `crates/krometrail-cdp/src/control/interaction.rs`, `crates/krometrail-cdp/src/control/tests.rs`.
- Tests added/removed: preserve-mode coverage proves no activation dispatch and recommends `focus: foreground`; foreground failure coverage proves bounded activation is reported accurately.
- Simplification: replaced one context-free hidden-target constructor with a policy-aware constructor.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-cdp --lib --locked hidden_pointer_target -- --nocapture`
