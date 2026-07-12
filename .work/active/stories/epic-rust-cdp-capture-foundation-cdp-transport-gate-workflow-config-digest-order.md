---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-workflow-config-digest-order
kind: story
stage: review
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Reproduce canonical configuration digest ordering in workflow

## Reproduction

Hosted macOS run `29211668813` at exact revision `365d02eaec088b954cabe65cab6b8a34a27d424d` completed the full gate and strict Rust normalization/decisive validation, then failed the redundant Python assertion at line 55. `canonical-config.json` pretty serialization orders keys alphabetically, while the canonical digest intentionally hashes compact struct-field order; Python reserialized the loaded map in file order and computed a different digest.

## Scope

Make the workflow assertion reproduce the Rust canonical digest using one explicit canonical field order or delegate digest verification entirely to the Rust canonical-config/decisive validator without duplicating ordering logic. Add a local test/script that executes the exact workflow assertion against generated canonical config and a valid synthetic/current report. Keep the redundant check fail-closed without creating a second digest source of truth.

## Acceptance criteria

- [x] Workflow canonical digest assertion matches Rust generation deterministically.
- [x] Reordering pretty JSON keys cannot change or falsely fail the canonical digest check.
- [x] Local workflow-contract regression executes the exact assertion path.
- [x] Candidate tests/clippy pass; no evidence/production/core change lands.

## Implementation notes

- Execution capability: inline implementation; the change is confined to the qualification CLI, workflow, and contract tests, with no parallel ownership needed.
- Review weight: standard, from the project default; caller explicitly requested the implementing-to-review boundary.
- Root cause: the workflow reserialized a JSON object whose generated pretty representation uses sorted `serde_json::Map` keys, while Rust hashes the `GateConfiguration` struct's declaration order.
- Fix: added the Rust-owned `verify-canonical-config` command and made the workflow call it; the Python contract now consumes the generated configuration and digest without recomputing or hard-coding ordering.
- Tests: added key-reordering and configuration-mutation regression tests, plus a valid synthetic strict-report workflow contract test. Ran the candidate tests and clippy with `-D warnings`.
- Files changed: `.github/workflows/cdp-transport-gate.yml`, `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs`, `crates/krometrail-cdp/tests/transport_contract.rs`.
- Adjacent issues parked: none.
