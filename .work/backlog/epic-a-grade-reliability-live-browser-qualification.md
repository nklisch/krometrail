---
id: epic-a-grade-reliability-live-browser-qualification
kind: feature
stage: backlog
tags: [browser, testing, infra]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Make live-browser coverage explicit and continuously exercise critical journeys

## Outcome and priority

Green deterministic tests do not establish that real browser behavior ran. Current opt-in test early returns obscure the distinction, and a manual transport lane does not cover the ordinary agent browsing/recovery journey.

- **Priority:** P2 — wave 3 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced coverage gap: opt-out early returns can appear as ordinary passing tests.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/qualification_support/chrome.rs:27 — real-browser opt-in
- .github/workflows/ci.yml — ordinary workspace tests
- .github/workflows/cdp-transport-gate.yml — manually dispatched qualification
- docs/EVALUATION.md — existing qualification and performance thresholds

## Acceptance criteria

- [ ] Report executed, skipped, blocked, failed, and inconclusive live cases separately; a requested required live lane cannot return a fake pass because Chrome or configuration is missing.
- [ ] Add a bounded automatic supported-browser smoke lane using local fixtures for lifecycle, navigation, discovery/selection, screenshot, same-document observation, interaction, and recovery.
- [ ] Add long-session target churn, range churn, input interruption, profile-owner termination, and resource cleanup qualification, including regressions from this review.
- [ ] Record platform/browser/binary/configuration identities and respect supported-platform claim boundaries; reuse existing platform evidence rather than treating Linux as macOS evidence.
- [ ] No paid model invocation enters ordinary CI. Deterministic/browser smoke success does not satisfy the separate product-thesis/model-effectiveness benchmark.

## Implementation direction and boundaries

Layer cheap automatic smoke, deterministic fault injection, and explicit longer qualification. Keep test-only opt-in controls distinct from truthful CI outcome reporting.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `epic-prove-temporal-advantage` — related authority/context, not an implicit blocking dependency.
- `epic-prove-temporal-advantage-platform-evidence-collection` — related authority/context, not an implicit blocking dependency.

## Qualification-feature compilation follow-up — 2026-09-05

Independent review of the discovery-only doctor branch (`7e0d83fe`/`3315bdba`, base `d5047192`) ran `cargo check -p krometrail --all-targets --features qualification-support --locked` and found a broken feature lane. Default workspace/root gates do not compile this feature and cannot establish its readiness.

The review separates newly removed runtime/storage projections still consumed by qualification code (owned by the doctor correction) from reportedly pre-existing stale `mcp_config` construction and operation matches in `src/app/live_evaluation.rs` and its modules. A baseline-versus-corrected compiler comparison has been requested; do not attribute all 79 reported errors to doctor or claim the lane passes merely after removing its introduced errors. The independent feature-check receipt is `/tmp/krometrail-doctor-independent-feature-check.cFV7l1.log`; preserve relevant diagnostics in this item when the differential is available.

Reconcile this feature-gated compilation debt as part of qualifying the existing harness, rather than deleting coverage or inventing a replacement harness. The current finding is a compilation/coverage blocker, not a failed live-browser run or evidence about browser behavior. No browser or paid model was invoked by this review.
