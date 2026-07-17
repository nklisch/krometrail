---
id: epic-agent-browser-reliability
kind: epic
stage: done
tags: [browser, agent-ux, storage, security]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Agent browser reliability

## Brief

Resolve the complete set of walkthrough findings reported in GitHub issues #1 through #12 and ship the result as one stable 1.x release. The work must make retained capture health, browser-control outcomes, post-operation evidence, target selection, structured references, keyboard/fill/pointer semantics, browser lifecycle, MCP schemas, responsive viewport control, and agent guidance agree as one coherent public contract.

The epic includes durable private diagnostics as the common debugging substrate. Failed and degraded operations must retain safe causal detail and a correlation path that agents can use from any working directory, while public responses remain actionable without log access and logs remain bounded, local, and free of browser content or secrets.

Implementation must preserve the stable-release contract: changes are additive or corrective, retained evidence remains readable, generated schemas and registry-derived surfaces stay single-source, and release assets/plugin activation continue to select the exact Cargo version. Every issue requires regression or contract evidence, the complete Rust quality gate, a fresh-context aggregate review, and release-helper publication only after the in-scope queue is terminal.

## Strategic decisions

- **Delivery boundary**: treat the twelve reports as one release-quality capability arc rather than twelve independent patches because several share public outcome, evidence, and targeting contracts.
- **Compatibility**: preserve existing valid requests and retained evidence; accept safer defaults and add diagnostic/result fields without removing stable 1.x shapes.
- **Capture posture**: control outcome, live observation, and retained temporal evidence are distinct facts and must be reported independently.
- **Viewport posture**: add explicit target-scoped viewport/device override and clear behavior; avoid an opaque preset registry until explicit metrics are proven.
- **Release posture**: complete and review the entire epic before invoking the repository release helper; do not bind individual items to a version early.

## Issue inventory

- #1 managed screencast failure and missing retained-capture diagnostics
- #2 control/navigation outcome conflated with observation or capture outcome
- #3 cold standard macOS Chrome discovery failure
- #4 hidden managed-page pointer focus recovery
- #5 false `shutdown_incomplete` after cleanup is complete
- #6 nested MCP schemas projected as `unknown`
- #7 replace-mode password fill appends on macOS
- #8 modifier chords and activation-key dispatch semantics
- #9 dark or partial post-interaction screenshots
- #10 first-class viewport resizing and device emulation
- #11 target defaults, reference lifetime, and off-screen click behavior
- #12 economical interaction-evidence guidance

## Simplification opportunity

Consolidate outcome classification, diagnostic correlation, target defaulting, element preparation, and schema projection at their existing boundaries. Remove issue-specific recovery folklore from the skill once the runtime can expose the real state directly.

## Design decisions

- **Feature boundaries**: split by caller-visible capability rather than crate layer so each feature owns one coherent contract and its integration evidence.
- **Diagnostic dependency**: land durable diagnostics before capture and lifecycle correction so newly reproduced failures retain causal evidence instead of repeating the current blind spot.
- **Guidance timing**: agent-facing schemas and skill guidance land after runtime behavior so examples and recovery advice describe the shipped contract.

## Decomposition

The decomposition keeps independent input and viewport work parallel while ordering causal diagnostics before capture/lifecycle repairs and ordering agent guidance after every runtime contract it documents. The durable-diagnostics child pre-existed from the initial scope pass; epic design retained it and filled the remaining capability gaps.

### Child features

- `durable-agent-diagnostics` — bounded private logs, sanitized causal events, correlation identifiers, and discoverable log location — depends on: `[]`
- `epic-agent-browser-reliability-capture-outcomes` — retained-capture health, action/evidence outcome separation, shutdown classification, and compositor-ready post-operation evidence for #1, #2, and #9 — depends on: `[durable-agent-diagnostics]`
- `epic-agent-browser-reliability-managed-session-lifecycle` — cold Chrome discovery, managed-page focus recovery, and truthful cleanup results for #3, #4, and #5 — depends on: `[durable-agent-diagnostics]`
- `epic-agent-browser-reliability-interaction-semantics` — platform-correct fill/key behavior, selected-target defaults, reference lifetime, and off-screen element preparation for #7, #8, and #11 — depends on: `[]`
- `epic-agent-browser-reliability-viewport-emulation` — explicit target-scoped viewport/device override and clear behavior for #10 — depends on: `[]`
- `epic-agent-browser-reliability-agent-contracts` — dereferenced MCP declarations, precise validation paths, and economical evidence/debugging guidance for #6 and #12 — depends on: `[durable-agent-diagnostics, epic-agent-browser-reliability-capture-outcomes, epic-agent-browser-reliability-managed-session-lifecycle, epic-agent-browser-reliability-interaction-semantics, epic-agent-browser-reliability-viewport-emulation]`

### Simplification arcs

- `durable-agent-diagnostics` centralizes tracing initialization and redaction instead of issue-specific stderr diagnostics.
- `epic-agent-browser-reliability-capture-outcomes` replaces conflated success/failure classification with one composable outcome model.
- `epic-agent-browser-reliability-managed-session-lifecycle` removes false cleanup alarms and implicit external-focus folklore.
- `epic-agent-browser-reliability-interaction-semantics` consolidates element preparation and key dispatch rather than maintaining selector/reference and platform-specific workarounds.
- `epic-agent-browser-reliability-agent-contracts` removes hand-maintained examples where generated schemas can remain authoritative.

### Decomposition risks

- Capture issue #1 is not reproducible against a clean temporary store; diagnostics must land first and the capture feature must preserve an honest unresolved-cause path until a failing boundary test identifies the rejected stage.
- Post-action compositor readiness must remain bounded so hidden/background targets cannot deadlock an interaction.
- Viewport emulation changes source-frame geometry mid-session; retained metadata and artifact normalization must continue to make that transition explicit.

## Aggregate review record

- Effective weight: standard; pass: 1; fresh-context verdict: request changes, resolved and approved by the integration owner.
- Finding 1 fixed: capture startup now observes native `devicePixelRatio` per target/attachment instead of assuming 1.0, while committed viewport transitions continue to update the same retained-frame authority. The decoder, constant-page-zoom scale transition, and opt-in cross-platform high-DPI capture qualification verify the boundary.
- Finding 2 fixed: a naturally exited browser leader no longer makes `terminate()` treat remaining process-group authority as complete. Group cleanup is rechecked when the direct child is already reaped; incomplete cleanup is retained for shutdown reporting, and the leader-exited/helper-survives regression passes.
- Aggregate verification: docs build; plugin static and managed-install smoke; both skill validators; `cargo fmt`, workspace check/test, strict clippy, runtime version/help/doctor, real Chrome interaction/viewport, and high-DPI capture qualification.
- Final verdict: approve.
