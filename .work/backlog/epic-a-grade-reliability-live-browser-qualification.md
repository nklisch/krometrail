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

## Recover the macOS smoke-workflow addition — 2026-09-05

The maintainer requested recovery of useful dangling work during repository cleanup. Retained branch `tmp/cross-platform-smoke-macos-595f079` contains workflow commit `a99d18e1c0bb8e07fed80c278cc05e59494bb21a`, whose 20-line addition to `.github/workflows/cdp-transport-gate.yml` is still absent from main at capture. It runs `cross_platform_smoke` with real Chrome enabled, requires default-DPI and high-DPI macOS JSON receipts, and uploads those receipts with the qualified commit identity. The branch's later smoke-test patch is already patch-equivalent to main; do not duplicate it or merge the entire old branch.

Review and adapt the workflow-only addition within this qualification outcome. Compare it against the current test target, opt-in behavior, feature-compilation blockers below, evidence format, action pins, and supported macOS runner before pulling it in. Preserve explicit failure when required receipts are absent and distinguish actual macOS execution from Linux evidence. This is a source-inspected recovery candidate, not a verified current workflow fix: no macOS run or fresh compilation was performed by cleanup.

Recovery source: `git show a99d18e1c0bb8e07fed80c278cc05e59494bb21a -- .github/workflows/cdp-transport-gate.yml`. Local investigation details are retained in `~/.cache/other-cleanup-result.md`; the commit and branch, not that machine-local report, are the source needed to implement this follow-up. Keep this capture under the existing live-browser qualification item rather than creating a competing CI outcome.

## Related existing work

- `epic-prove-temporal-advantage` — related authority/context, not an implicit blocking dependency.
- `epic-prove-temporal-advantage-platform-evidence-collection` — related authority/context, not an implicit blocking dependency.

## Qualification-feature compilation follow-up — 2026-09-05

Independent review of the discovery-only doctor branch (`7e0d83fe`/`3315bdba`, base `d5047192`) ran `cargo check -p krometrail --all-targets --features qualification-support --locked` and found a broken feature lane. Default workspace/root gates do not compile this feature and cannot establish its readiness.

The review separates newly removed runtime/storage projections still consumed by qualification code (owned by the doctor correction) from reportedly pre-existing stale `mcp_config` construction and operation matches in `src/app/live_evaluation.rs` and its modules. The worker's baseline-versus-corrected compiler comparison was independently checked during re-review: both have the same 11 error signatures and locations. The 68 errors introduced by the initial doctor dependency trim are gone. Do not attribute the baseline failures to doctor or claim this lane passes. The independent feature-check receipt is `/tmp/krometrail-doctor-independent-feature-check.cFV7l1.log`; the differential logs are `/tmp/kt-feature-baseline.log` and `/tmp/kt-feature-corrected.log`.

The 11 remaining errors comprise seven E0308 type mismatches in qualification control/latency/recovery/retention code, three E0004 non-exhaustive browser-operation matches, and one E0560 stale `mcp_config` initializer in `src/app/live_evaluation.rs`. Corrected qualification compilation also emits a non-test dead-code warning: equal error signatures do not mean identical diagnostics. These are explicit baseline compilation blockers, not passing qualification.

Reconcile this feature-gated compilation debt as part of qualifying the existing harness, rather than deleting coverage or inventing a replacement harness. The current finding is a compilation/coverage blocker, not a failed live-browser run or evidence about browser behavior. No browser or paid model was invoked by this review.

## Installed browser smoke — 2026-09-06

After the user updated/reloaded the plugin following the empty-adapter-directory repair, the actual conversation's `krometrail-browser` MCP tools completed a bounded Linux smoke. `browser_status` identified the running server as **1.6.3**; Chrome reported **151.0.7922.137**. This is live installed-runtime evidence, not a fresh-consumer SDK substitute.

Executed successfully: temporary-profile launch with an attached selected page; public example-domain screenshot with a native inline image and structured metadata both visible; semantic link lookup and reference-based click with observed navigation to IANA; responsive-small viewport with an observed 390×844 image; second-page creation/listing and follow-up screenshot; closing that selected page with automatic selection returning to the original page; navigation to a disposable local data-document fixture; reference-based fill and button click with the resulting status confirmed semantically and visually. No account, clipboard, download, or external form submission was used.

Session `eb224819-20cd-4d58-b49d-6d835a92dae5`, original target `76fa7167-1b0c-42da-b9ed-06bb8f9d23f6`. At the capture-health check, 27 frames were received and persisted, zero were dropped, and one session-level gap was reported. Resolving the final button interaction `89f2d108-4833-400a-8ec0-22df2bdeeea5` with a ±1-second window found four frames and no gaps within that interval, but explicitly reported a partial requested tail beyond the newest retained frame. Do not claim uninterrupted capture or full-window coverage.

Retained frame `15a0169a-bb4a-47dc-b606-1afde84421ea` was fetched and visually inspected; JPEG 390×844, 10,498 bytes, SHA-256 `ac927fd0dcc94fe72ea8d2ac7b0ef05c9c7fa63f4721ba939fe479bb0bc3ab56`. The managed browser then stopped cleanly, and the same retained frame was fetched again with the same hash after stop. Canonical URI: `krometrail://evidence/eb224819-20cd-4d58-b49d-6d835a92dae5/76fa7167-1b0c-42da-b9ed-06bb8f9d23f6/frames/15a0169a-bb4a-47dc-b606-1afde84421ea` (subject to retention).

Qualification limits: new-page creation returned `degraded` with `compositor_rendezvous_unobserved`, correlation `2746818a-c148-49b0-8eaa-785b781ba73c`; its follow-up screenshot succeeded. This is a reported bounded-readiness warning, not a failed page attachment. One agent-authored screenshot request omitted required arguments and was rejected; the corrected schema-valid request passed. The earlier zero-page/unattached failure did not reproduce in this run, but its cause is not established. No macOS/Windows, sustained stress, crash recovery, clipboard permissions, video, or optional qualification-harness coverage is claimed. The temporary browser was closed.
