---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-wire-authenticity-remediation, epic-rust-cdp-capture-foundation-cdp-transport-gate-deadline-observation-remediation]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make decisive evidence and decision provenance platform-faithful

## Origin

Phase 2 feature review found materially different Linux/macOS evidence contracts, optional RSS cadence fields and a legacy alias, and a decision gate list copied only from Linux.

## Scope

Version the evidence contract if required. Require canonical RSS sample, cadence, and warmup fields on every decisive platform report. Remove the compatibility alias from decisive validation. Bind candidate-contract traces to reports where gates depend on scripted evidence. Make the decision contain platform-labelled gate results or a documented conservative aggregate that cannot hide worse measurements. Reject reports from different gate implementation revisions/configurations. Preserve exact report-byte digests and strict redaction.

## Acceptance criteria

- [ ] Linux and macOS decisive reports must use one canonical complete evidence contract and one immutable gate implementation revision.
- [ ] Decision output preserves both platforms' results or explicit worst-case aggregation with provenance.
- [ ] Scripted candidate evidence is trace-bound rather than silently represented as real-Chrome measurement.
- [ ] Schema/normalization/decision regression tests reject aliases, omitted cadence fields, mixed revisions, and Linux-only rollups.
