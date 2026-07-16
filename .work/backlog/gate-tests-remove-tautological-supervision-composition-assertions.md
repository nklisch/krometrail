---
id: gate-tests-remove-tautological-supervision-composition-assertions
created: 2026-07-15
updated: 2026-07-15
tags: [testing, cleanup]
gate_origin: tests
release_binding: null
---

Remove or replace two low-value assertions: `session_supervision.rs` constructs `ConnectionLost` and only matches the value it just constructed, while `src/app.rs` compares an `Arc` pointer with itself rather than the service recipients. Preserve the stronger reducer, session-capture, and concrete dependency-identity tests already covering the meaningful contracts. This is ambient test cleanup and does not block 1.0.
