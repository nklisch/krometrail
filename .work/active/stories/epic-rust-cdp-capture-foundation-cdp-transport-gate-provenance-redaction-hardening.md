---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-provenance-redaction-hardening
kind: story
stage: implementing
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

- [ ] Decisive reports require valid exact SHA provenance and a clean relevant-tree attestation before/after execution.
- [ ] Hosted and local runs expose reproducible raw/sanitized provenance; final Linux capture uses an immutable exact-SHA checkout or equivalent clean attestation.
- [ ] Recursive redaction and status/failure consistency reject all reproduced bypasses.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or fabricated evidence lands.
