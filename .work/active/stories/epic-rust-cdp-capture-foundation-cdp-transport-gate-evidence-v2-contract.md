---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-wire-authenticity-remediation, epic-rust-cdp-capture-foundation-cdp-transport-gate-deadline-observation-remediation]
release_binding: 1.0.0
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

- [x] Linux and macOS decisive reports must use one canonical complete evidence contract and one immutable gate implementation revision.
- [x] Decision output preserves both platforms' results or explicit worst-case aggregation with provenance.
- [x] Scripted candidate evidence is trace-bound rather than silently represented as real-Chrome measurement.
- [x] Schema/normalization/decision regression tests reject aliases, omitted cadence fields, mixed revisions, and Linux-only rollups.

## Implementation notes

- Versioned the generated evidence contract to schema 2. Decisive reports now require immutable gate implementation revision, configuration digest, fixture identity, canonical RSS samples/cadence/warmup, observed lifecycle measurements, and a complete candidate-contract trace/hash/results object.
- Removed legacy `rss_sample_count` acceptance and all optional RSS cadence/warmup normalization. Decision output now stores platform-labelled gates and candidate-contract results for both Linux and macOS; it cannot expose a Linux-only gate rollup.
- The scripted candidate contract runs the shared wire-observed scenario suite and hashes the serialized observation trace. Results are derived from that trace/scenario evidence and are bound into every real-Chrome report that uses them.
- Added regression coverage for stale schema, legacy aliases, RSS omissions, stale configuration provenance, mixed gate revisions, platform-labelled decisions, and historical report rejection.
- Generated `docs/evidence/cdp-transport/v2/schema.json` and added its requalification README. Retained v1 reports/decision are unchanged and explicitly obsolete; no replacement reports were fabricated and no current transport selection is claimed.
- No production, root, or core files changed. Verification passed: `cargo fmt --all --check`; default workspace tests/clippy; spike tests/clippy; and cdpkit-feature tests/clippy.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane contract review verified strict schema v2, canonical RSS/deadline fields, trace-bound candidate evidence, same-revision/config enforcement, platform-labelled decision results, historical v1 rejection, 22 candidate-feature tests, and denied-warning clippy. No current selection is claimed before requalification. Verdict: Approve - story verified by implement; fast-lane advance.
