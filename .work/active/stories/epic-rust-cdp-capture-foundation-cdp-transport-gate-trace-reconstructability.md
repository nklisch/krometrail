---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-trace-reconstructability
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: 1.0.0
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

- [x] Decisive validation recomputes digest/count/results from committed canonical trace material.
- [x] Fixture payload and all candidate result mutations are rejected without trusting duplicated summary fields.
- [x] Linux/macOS decisions require identical deterministic canonical trace material.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.

## Implementation notes

- Execution capability: inline implementation; the story is confined to the disposable `krometrail-cdp` spike evidence boundary, generated schema, tests, workflow assertions, and documentation, with no subagents or questions as requested.
- Review weight: standard; the caller explicitly requested the implementation stop at `stage: review`.
- Files changed: `crates/krometrail-cdp/src/spike/{contract,evidence,mod,scenarios,scripted_peer}.rs`, spike contract tests, `.github/workflows/cdp-transport-gate.yml`, generated `docs/evidence/cdp-transport/v2/schema.json`, and current contract documentation.
- Tests added: exact all-zero digest, fabricated `routing_commands=201`, fixture params/order, lifecycle, and decision revalidation regressions; canonical trace round-trip/schema coverage.
- Discrepancies from design: none. The canonical fixture digest is now the stable ordered digest of the parsed fixture projection, while the historical raw fixture/report bytes remain untouched.
- Adjacent issues parked: none.
- Historical reports were not edited and no fresh browser evidence was generated.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane evidence review verified canonical bounded trace material, recomputed fixture/trace/result summaries, cross-platform material equality, explicit rejection of zero digest/fabricated routing/fixture/lifecycle mutations, 35 candidate-feature tests, and denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
