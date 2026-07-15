---
id: configurable-capture-cadence
kind: feature
stage: drafting
tags: [browser, agent-ux, visual, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Configurable Capture Cadence

## Brief

Give the person or agent that starts a browser session one explicit, typed control over the relative visual-capture stride. `every_nth_frame` is accepted on both `LaunchBrowser` and `AttachBrowser`, exposed through the generated MCP `start_browser` and `attach_browser` schemas, validated at the external boundary, and frozen for the lifetime of that browser connection/recording session. The value is an integer from 1 through 60 and defaults to 1, preserving maximum-evidence behavior when omitted.

The stride is a best-effort relative sampling request, not an exact frame rate. The CDP adapter passes it directly to `Page.startScreencast.everyNthFrame`. Session and capture status, evaluation manifests, and accepted-claim provenance record the requested stride so reduced capture probability is visible to humans and agents. Deliberate stride selection remains distinct from ordinary queue drops, persistence failures, visibility gaps, and other capture-gap causes.

## Strategic decisions

- **Configuration authority**: capture stride is a per-connection/per-recording-session request; there is no mid-stream mutation or stop/restart path in v1, because changing it would require an explicit capture-gap contract.
- **Public contract**: core launch and attach request types are the single domain contract; MCP lifecycle schemas are generated from those types. No CLI command, environment variable, configuration-file setting, alias, shim, or legacy reader is added for this capability.
- **Semantics**: `every_nth_frame` is a bounded relative stride (1..=60), not an FPS promise. The requested value is provenance, while observed cadence and known loss remain separate evidence.
- **Audience**: agents and humans using an MCP client receive the same field and validation behavior.

## Code boundary map

- `crates/krometrail-core/src/ports/browser.rs`: extend the clean prepublic `LaunchBrowser` and `AttachBrowser` request contracts with one shared typed stride value and boundary validation/defaulting; keep the field available to generated JSON Schema.
- `crates/krometrail-core/src/recording/session.rs` and `src/browser/control.rs`: carry the immutable requested stride through recording/session and browser/capture status projections without creating a second configuration authority. Preserve it in serialized status and event-facing capture status.
- `crates/krometrail-cdp/src/capture/mod.rs` and `capture/pipeline.rs`: thread the session value into the capture assembly/runtime and replace the hard-coded CDP `everyNthFrame: 1` start parameter. Do not infer continuity or gap counts from the stride.
- `crates/krometrail-cdp/src/session/mod.rs` and `src/app.rs`: bind the validated launch/attach request to one session capture configuration at connection composition time; do not allow later target attachment or reconnect paths to select a different stride.
- `crates/krometrail-mcp/src/registry.rs` and `session.rs`: rely on generated `LaunchBrowser`/`AttachBrowser` schemas and existing lifecycle routing so `start_browser` and `attach_browser` expose the same field and invoke the same validation path.
- `crates/temporal-evaluation/src/manifest.rs`, `src/app/live_evaluation/report.rs`, and capture qualification evidence helpers: record the requested stride in capture configuration identity and bind evaluation rows/accepted claims to that identity, so a claim is never read as evidence of a full-cadence stream when a deliberate stride was requested.
- Existing capture/status and qualification tests around the listed seams should cover defaulting, bounds, CDP parameter forwarding, immutable reconnect behavior, status serialization, generated MCP schemas, and provenance/claim traceability. Real Chrome is not needed for scope or design.

## Simplification opportunity

Use the existing request, generated-schema, session-status, and capture-configuration paths instead of adding a parallel global setting or a new cadence service. Remove the hard-coded `everyNthFrame: 1` assumption and reuse the existing `CaptureConfig`/status/provenance authorities. Do not change queue accounting, capture-gap semantics, ordinal meaning, or observed-cadence measurements to make deliberate stride look like transport loss. No unrelated CLI/configuration work is included.

## Dependencies and cycle check

No active item is a prerequisite: the existing browser lifecycle, MCP control-surface, CDP capture, and evaluation features provide the seams this feature extends, and none needs to be made a dependency for design to begin. Cycle check ran with `.work/bin/work-view --blocking configurable-capture-cadence --paths`; it reported no blockers or dependency paths.

## Foundation roll-forward

`docs/SPEC.md` replaces the unconditional every-available-frame capture assertion with the session-scoped best-effort `every_nth_frame` contract and makes requested stride, observed cadence, and ordinary capture gaps distinct. `docs/ARCHITECTURE.md` records the core-to-MCP-to-CDP flow, immutable session ownership, status/provenance propagation, and the absence of a parallel CLI or process configuration authority.

## Dispatch note

Direct-read scope mapping only; the boundaries and existing hard-coded CDP parameter are known. No exploratory agent, Chrome run, network access, or generated-artifact change is needed at scope time.
