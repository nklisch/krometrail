---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Supervise target sessions, reconnect, and shutdown

## Scope

Implement Unit 4 of the parent design: the fully defined `SupervisorState`/`SupervisorInput`/`SupervisorEffect` reducer and `TransportTargetInfo`/`ReconnectedSnapshot` boundary, flat discovery/auto-attach reconciliation, session event publication, target-local failure isolation, finite reconnect/rebuild, explicit cancellation and ownership-correct shutdown, root connector wiring, truthful discovery-only `doctor`, architecture roll-forward, and real-browser integration evidence.

Consume the serialized contracts, transport, and launcher stories. `ProductionBrowserConnector::installations()` delegates directly to `ChromeLauncher::installations()`; only `launcher/discovery.rs` owns discovery precedence. `BrowserSessionState` describes browser connectivity and is not the persisted recording `SessionLifecycle`; any orchestration mapping between them is explicit rather than type reuse.

Managed `BrowserProcessTerminated` publishes `SessionFailed(browser_process_terminated)`, skips reconnect/relaunch, and performs bounded owned cleanup. `ConnectionLost` enters reconnect only while a managed child is alive or an attached endpoint remains eligible.

Outbound event fan-out is bounded and revisioned. Only observable subscriber-channel overflow is measurable: report the missed revision range, increment the outbound lag counter, and require `targets()` refresh. Do not claim or infer cdpkit private upstream queue depth.

Do not implement production screencast start/events/acknowledgement, frame queues, persistence, capture gaps, browser actions, or snapshots.

## Required files

- `crates/krometrail-cdp/src/targets/mod.rs` (new)
- `crates/krometrail-cdp/src/targets/model.rs` (new)
- `crates/krometrail-cdp/src/targets/reducer.rs` (new)
- `crates/krometrail-cdp/src/targets/supervisor.rs` (new)
- `crates/krometrail-cdp/src/session.rs` (new)
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs`
- `src/cli.rs`
- `tests/rust-runtime-smoke.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs` (new)
- `crates/krometrail-cdp/tests/support/static_fixture.rs` (new)
- `crates/krometrail-cdp/tests/target_reducer.rs` (new)
- `crates/krometrail-cdp/tests/session_supervision.rs` (new)
- `crates/krometrail-cdp/tests/chrome_session_real.rs` (new)
- `docs/ARCHITECTURE.md`

`crates/krometrail-cdp/src/lib.rs` is intentionally a third serialized edit to export session/targets; no story sharing it can run concurrently.

## Doctor smoke contract

Replace `doctor_reports_unavailable_browser_transport` in `tests/rust-runtime-smoke.rs`. The new smoke accepts exactly two environment-dependent production outcomes: success with a stable `browser available:` summary for one or more discovered installations, or exit 1 with stable `browser_not_found` and recovery text when discovery is empty. It rejects the provisional `unsupported`/`browser transport is not available` message. An `src/app.rs` unit fake records one `installations()` call and makes `connect()` panic, proving doctor performs discovery only and does not launch, attach, allocate a port, or acquire a profile.

## Acceptance criteria

- [ ] Reducer tests directly construct the parent-defined supervisor/input/effect/target/reconnect types; subscription-before-enable plus snapshot reconciliation is idempotent across creation/attach/info/detach/destroy races, and only recordable page targets are published.
- [ ] The complete `Suspended` transition table is exercised for every legal restoration and every illegal edge. Pre-suspension lifecycle is retained, exact browser target keys preserve `TargetId`, attachment generations reject stale events, and changed/missing keys close/create rather than URL/title-match.
- [ ] Target-local failures leave unrelated targets/session alive. Transport loss enters finite reconnect; managed process termination instead emits `browser_process_terminated` and never reconnects or relaunches. Exhaustion/cancellation ends with its own typed error and bounded cleanup.
- [ ] Initial visibility is probed and later visibility signals update the reducer without starting a screencast. Observable slow-subscriber revision lag cannot backpressure supervisor state; no unmeasurable upstream-lag assertion exists.
- [ ] Managed stop closes/terminates only the owned browser and cleans profile correctly; attach stop detaches and leaves the external browser/Electron app alive.
- [ ] Real Chrome against `tests/fixtures/browser/cdp-transport-gate` proves managed launch, attach-without-close, two isolated targets, disconnect/rebuild, and no process/profile leak. Electron has mandatory deterministic capability coverage plus an opt-in real endpoint test.
- [ ] Root uses the default-feature production connector; connector installation discovery delegates to `ChromeLauncher`. Doctor satisfies the smoke contract above.
- [ ] Supervisor tracing covers session state/reconnect and target discovered/attached/changed/suspended/closed/failed with the parent's privacy fields. Transport compatibility and launcher discovery/launch/shutdown tracing are verified in stories 2 and 3, not deferred here.
- [ ] Workspace format/check/test/clippy, real-browser command, default/no-default production feature checks, spike regression, and dependency scans pass; architecture reflects the final5 production boundary.
- [ ] No production screencast ingestion lands.
