---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-transport-decision-rollup
kind: story
stage: review
tags: [browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-decisive-evidence]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Select and roll forward the evidence-backed transport

## Scope

Validate the Linux and macOS evidence set, compute the transport decision without weakening any gate, and roll the selected mechanism and its limitations into the machine-readable decision, research reference, versioned skill, feature, parent epic, and architecture technology decision. Do not implement the selected production adapter or revise core ports unless the evidence explicitly proves the current boundary unavoidable to change; any such implementation remains later work.

## Exact files

- `docs/evidence/cdp-transport/v1/decision.json`
- `docs/evidence/cdp-transport/v1/README.md`
- `docs/research/rust-cdp-transport-2026-07.md`
- `.agents/skills/rust-cdp-transport/SKILL.md`
- `.work/active/features/epic-rust-cdp-capture-foundation-cdp-transport-gate.md`
- `.work/active/epics/epic-rust-cdp-capture-foundation.md`
- `docs/ARCHITECTURE.md`

## Requirements

- Validate schema version, required gate completeness, report digests, candidate/version consistency, platform identity, thresholds, and redaction before selecting.
- `decision.json` records `AdoptCdpkit`, `AdoptChromey`, or `OwnTransport`; candidate/version, Linux/macOS evidence paths and SHA-256 digests, every gate result, limitations, rejected alternatives, and rationale. It must be derivable from the evidence reports rather than a hand-maintained contradictory gate list.
- Adopt exact cdpkit 0.4.0 only if all mandatory fake, Linux, and macOS gates pass unchanged. A fork or required routing/decoder/lifecycle patch is failure.
- Consider chromey only after a documented cdpkit lifecycle, ordering, or sustained-capture failure that its handler could plausibly address. Select the minimal owned transport when either library loses evolution before the raw boundary, obscures prompt ack/backpressure, misroutes sessions, or requires a fork.
- State cdpkit's named raw event-params limitation exactly. Do not call it wildcard/full-envelope compatibility and do not silently weaken any foundation requirement; if full-envelope preservation is necessary, cdpkit fails that requirement and selection follows the fallback rules.
- Roll the result forward in place. The research doc and skill cease saying “no selection”; the feature and epic record the decision/evidence and conditional work actually created; `docs/ARCHITECTURE.md` names the selected adapter while preserving replaceability and Krometrail-owned reconnect/capture policy.
- Keep spike feature flags non-default after the decision for reproducibility. They do not become the production adapter and are not wired into the root binary.

## Acceptance criteria

- [x] Machine-readable decision and both platform reports validate, hash-match, contain no secret/path leakage, and reproduce from documented commands.
- [x] The selected transport follows the published decision rules with no waived or missing gate.
- [x] Evidence, research, skill, feature, epic, and architecture agree on the selected mechanism, exact version/provenance, limitations, and fallback reasoning.
- [x] The existing core port remains unchanged unless an evidence-cited incompatibility makes revision unavoidable; no production lifecycle/capture implementation lands.
- [x] Default and spike-feature Rust quality gates pass, and all child work—including any late-bound fallback story—is at review or done before this story advances.

## Implementation notes

- Execution capability: highest-tier direct implementation; the caller prohibited questions and subagents, and the decision/evidence/docs surface required one owner to preserve a single source of truth.
- Review weight: maximum requested by the active autopilot caller; this handoff stops at `stage: review` as requested.
- Files changed: `crates/krometrail-cdp/src/spike/evidence.rs`, `crates/krometrail-cdp/src/spike/mod.rs`, `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs`, `crates/krometrail-cdp/tests/transport_contract.rs`, `docs/evidence/cdp-transport/v1/decision.json`, `docs/evidence/cdp-transport/v1/README.md`, `docs/research/rust-cdp-transport-2026-07.md`, `.agents/skills/rust-cdp-transport/SKILL.md`, `.work/active/features/epic-rust-cdp-capture-foundation-cdp-transport-gate.md`, `.work/active/epics/epic-rust-cdp-capture-foundation.md`, and `docs/ARCHITECTURE.md`.
- Tests added: committed-report decision/digest/threshold regression coverage; the existing strict evidence, schema, fake, and candidate contract suites remain green.
- Discrepancies from design: the committed reports use `rss_sample_count` in the sustained gate while the generated contract historically used `rss_samples`; validation now accepts that report-specific alias without weakening the bounded-memory gate, whose canonical field remains `rss_samples`.
- Adjacent issues parked: none.
- Dispatch rationale: direct-read only; no subagent or question dispatch was used per caller instruction.
- Production boundary: no adapter/root wiring/core-port revision landed. The spike remains non-default. Reconnect, bounded handoff/backpressure, capture gaps, cancellation, and flush remain Krometrail-owned.
