---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-drift-trace-authenticity
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

# Authenticate protocol drift fixtures and candidate trace results

## Origin

Second adversarial feature review found that drift scenarios assert only wrapper method/scope, never exact raw params, and do not consume the committed fixtures. It also found nominal lifecycle values in candidate results and no cross-platform trace equality requirement.

## Scope

Load the committed unknown-event/additive-field/unknown-enum fixtures into the scripted wire server. Assert exact params received through cdpkit and derive fixture count/methods from recorded observations. Bind an ordered protocol-fixture digest into candidate evidence. Derive every wire-observable result directly from observations; explicitly classify non-wire runtime assertions. Require identical deterministic candidate-contract evidence (trace, fixture digest, results) across decisive platform reports.

## Acceptance criteria

- [ ] Drift passes only when exact committed fixture parameters survive the candidate path.
- [ ] Fixture count/digest and wire-observable results derive from the recorded trace; no nominal values masquerade as trace-derived.
- [ ] Decision rejects any Linux/macOS candidate trace, fixture digest, or deterministic-result mismatch.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.
