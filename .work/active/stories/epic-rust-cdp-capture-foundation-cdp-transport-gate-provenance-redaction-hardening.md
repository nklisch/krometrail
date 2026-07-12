---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-provenance-redaction-hardening
kind: story
stage: review
tags: [bug, browser, infra, security, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make qualification provenance and redaction fail closed

## Origin

Second adversarial feature review proved that arbitrary revision strings pass, local qualification does not attest a clean relevant tree, redaction misses HTTP/private paths/credentials and gate failures, and passing gates may contain failures.

## Scope

Require lowercase 40-hex source and implementation revisions. Before and after qualification, attest that manifests, lockfile, spike code/tests, workflow, and fixtures match the expected committed revision with no relevant tracked or untracked changes; record/validate an attestation digest. Strengthen report consistency and recursive string redaction across all fields including failures. Reject pass+failure, fail-without-failure, HTTP/WS URLs, IPv4/IPv6 endpoints, broad absolute/user/private paths, generic credential assignments, usernames, and secret-bearing failure text while preserving legitimate revision/digest strings. Add adversarial regressions.

## Acceptance criteria

- [x] Decisive reports require valid exact SHA provenance and a clean relevant-tree attestation before/after execution.
- [x] Hosted and local runs expose reproducible raw/sanitized provenance; final Linux capture uses an immutable exact-SHA checkout or equivalent clean attestation.
- [x] Recursive redaction and status/failure consistency reject all reproduced bypasses.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or fabricated evidence lands.

## Implementation notes

- Added lowercase full-SHA validation and deterministic source attestation over the gate manifests, lockfile, binary, spike sources/tests, workflow, and browser fixtures. The gate attests before and after execution; decisive validation recomputes the attestation against the clean checkout.
- Bound the attestation and configuration digests to gate provenance, retained expected revision provenance on failure reports, and wired the CLI/workflow to reject uppercase or short revisions.
- Made serialized evidence validation recursive across every string, including `SpikeError` failure payloads, rejecting URLs, HTTP/WS endpoints, IPv4/IPv6 endpoints, private/absolute paths, and sensitive assignments while retaining legitimate SHA/digest identities. Enforced status/failure and gate binding consistency.
- Regenerated `docs/evidence/cdp-transport/v2/schema.json` and documented the new contract without modifying historical evidence JSON.

Verification: `cargo test --workspace`; `cargo test -p krometrail-cdp --features cdp-spike`; `cargo test -p krometrail-cdp --features cdp-spike-cdpkit`; `cargo clippy --workspace --all-targets -- -D warnings`; feature-specific clippy for `cdp-spike` and `cdp-spike-cdpkit`; schema generation/check; `git diff --check`.
