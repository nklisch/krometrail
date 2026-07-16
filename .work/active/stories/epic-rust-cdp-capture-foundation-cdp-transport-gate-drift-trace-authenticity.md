---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-drift-trace-authenticity
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

# Authenticate protocol drift fixtures and candidate trace results

## Origin

Second adversarial feature review found that drift scenarios assert only wrapper method/scope, never exact raw params, and do not consume the committed fixtures. It also found nominal lifecycle values in candidate results and no cross-platform trace equality requirement.

## Scope

Load the committed unknown-event/additive-field/unknown-enum fixtures into the scripted wire server. Assert exact params received through cdpkit and derive fixture count/methods from recorded observations. Bind an ordered protocol-fixture digest into candidate evidence. Derive every wire-observable result directly from observations; explicitly classify non-wire runtime assertions. Require identical deterministic candidate-contract evidence (trace, fixture digest, results) across decisive platform reports.

## Acceptance criteria

- [x] Drift passes only when exact committed fixture parameters survive the candidate path.
- [x] Fixture count/digest and wire-observable results derive from the recorded trace; no nominal values masquerade as trace-derived.
- [x] Decision rejects any Linux/macOS candidate trace, fixture digest, or deterministic-result mismatch.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.

## Implementation notes

- Loaded the three ordered committed protocol fixtures from their exact bytes into the scripted WebSocket server; the cdpkit scenario now asserts exact method, `session-a` scope, and full params, including the additive `new_field` and future enum value.
- Added the ordered fixture-byte digest to candidate evidence and projected drift count/methods, routing, ordering, detach, socket, reconnect, and rebuilt-session values from recorded observations. Candidate close-status assertions are carried in a typed `runtime` section and are not labelled as wire observations.
- Decision assembly now rejects any cross-platform candidate fixture digest, trace hash, or complete deterministic result mismatch. The generated schema and v2 evidence README describe the new contract; existing v2 reports are historical/obsolete and were not edited or regenerated as evidence.
- Verification: `cargo fmt --all`; default, spike, and cdpkit test suites; denied-warning clippy for spike and cdpkit features. No production/core code or evidence JSON changed.
- Restored `.work/bin/work-view` before handoff; `.pi/` remains ignored.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane review verified exact committed fixture params through cdpkit, ordered fixture-byte digest, observation-derived wire results, typed runtime assertions, cross-platform complete-contract equality, 26 candidate-feature tests, and denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
