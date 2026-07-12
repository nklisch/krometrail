---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-trace-reconstructability
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make candidate trace evidence reconstructable

## Origin

Final adversarial review proved decisive validation accepts a fabricated trace digest and mutated results because reports carry only a digest/count/results, not canonical trace material from which they can be recomputed.

## Scope

Preserve sanitized canonical trace material (or a compact sufficient canonical projection) in decisive evidence. During report and decision validation, recompute trace digest, observation count, exact fixture payload claims, routing/order/lifecycle wire results, and typed runtime assertions from the committed material. Reject any mismatch, including all-zero digest or changed command counts. Keep evidence bounded and machine-neutral; strict redaction applies recursively.

## Acceptance criteria

- [ ] Decisive validation recomputes digest/count/results from committed canonical trace material.
- [ ] Fixture payload and all candidate result mutations are rejected without trusting duplicated summary fields.
- [ ] Linux/macOS decisions require identical deterministic canonical trace material.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.
