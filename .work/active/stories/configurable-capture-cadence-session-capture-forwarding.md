---
id: configurable-capture-cadence-session-capture-forwarding
kind: story
stage: done
tags: [browser, visual, testing]
parent: configurable-capture-cadence
depends_on: [configurable-capture-cadence-core-contracts-and-status]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Bind the requested stride to one CDP capture session authority

## Checkpoint

Carry the validated core value from `BrowserConnectRequest` into one immutable production session
capture authority, forward it to every `Page.startScreencast` generation, and prove reconnect
preservation using scripted CDP. Keep process-wide capture operations and transport target
identity separate from this session request.

## Exact implementation

**Files**:

- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`
- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`
- `crates/krometrail-cdp/tests/session_capture.rs`
- `crates/krometrail-cdp/tests/session_supervision.rs`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- existing CDP evidence fixtures that serialize effective capture configuration

Leave the public adapter `CaptureConfig` unchanged: its format, queue, payload, timeout, and
shutdown fields remain the global operational assembly. Add `EveryNthFrame` only to the private
`CaptureCoordinator`, pass it into each `StreamRuntime`, and expose a read-only coordinator getter
for status projection. Extract the request value once in `ProductionBrowserConnector::connect`,
bind it to `SessionShared` and the coordinator, and never reconstruct it during reconnect.

Build `Page.startScreencast` params without cadence in the JPEG/PNG match, then insert one common
`everyNthFrame` property from `EveryNthFrame::get()`. Keep `CaptureEffectContext` unchanged. The
existing reducer continues to decide which exact target/connection/attachment/transport context
starts, suspends, resumes, or stops; only the session-owned coordinator supplies cadence.

Every `TargetCaptureStatus` emitted by the capture pipeline carries the coordinator value. Every
`BrowserStatus` carries the original request-bound value, including sessions constructed in tests
without a capture assembly. No setter, mutation command, restart path, or fallback to a global
stride is introduced.

## Acceptance evidence

- [x] Scripted CDP observes the requested `everyNthFrame` for both JPEG and PNG starts; production
      code has no hardcoded `everyNthFrame: 1` parameter.
- [x] Initial and dynamic target starts use the same session value, while existing subscribe-before-
      start and ack-first ordering remain intact.
- [x] A scripted physical transport disconnect/reconnect sends the same requested value on the new
      connection, preserves target identity and attachment-generation rules, and does not emit a
      stride-derived capture gap.
- [x] Status and capture-state event values equal the request value; observed cadence, queue drops,
      persistence failures, visibility gaps, ordinals, and gap reasons remain independent.
- [x] Tests use existing fake/scripted transport seams only. No real Chrome, model, network, or
      sleep-based timing is needed.

## Ordering

Depends on the core type/status checkpoint. MCP schema forwarding follows this checkpoint in the
feature's sequential graph; evaluation consumes the completed public status identity.

## Implementation notes

- Execution capability: direct-read inline implementation as feature owner.
- The validated request value is copied once before consuming `BrowserConnectRequest`, stored on
  immutable `SessionShared` and `CaptureCoordinator`, and copied into every `StreamRuntime` generation.
  `CaptureEffectContext` and public `CaptureConfig` remain unchanged.
- `Page.startScreencast` now builds JPEG/PNG format parameters first and inserts one common
  `everyNthFrame` value. Capture status projections use the session-owned value without touching
  acknowledgement, ordinal, queue, persistence, cadence, or gap logic.
- Tests cover request extraction for launch and attach, no-assembly status forwarding, JPEG/PNG wire
  parameters, subscribe-before-start, initial/dynamic starts, target identity and attachment
  generation across a physical scripted reconnect, capture-state event projection, and the absence
  of stride-derived gaps.
- Verification: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`,
  `cargo test --workspace --all-targets --locked` (721 passed, 4 ignored), and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` all pass.
