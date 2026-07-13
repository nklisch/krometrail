---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor
kind: story
stage: review
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

Consume the serialized contracts, transport, and launcher stories. The contracts story already owns a compile-real edit of `src/app.rs` and `tests/rust-runtime-smoke.rs`: an empty-installations transitional `UnavailableBrowserConnector` and stable discovery-only `browser_not_found` behavior. This story edits those files sequentially after that dependency, replacing the unavailable root composition with `ProductionBrowserConnector` and broadening the same smoke to production discovery outcomes. Remove the transitional connector/composition rather than retaining it as a stale fallback or compatibility path. `ProductionBrowserConnector::installations()` delegates directly to `ChromeLauncher::installations()`; only `launcher/discovery.rs` owns discovery precedence. `BrowserSessionState` describes browser connectivity and is not the persisted recording `SessionLifecycle`; any orchestration mapping between them is explicit rather than type reuse.

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
- `src/app.rs` (sequential replacement of the contracts-story transition)
- `src/cli.rs`
- `tests/rust-runtime-smoke.rs` (sequential broadening of the contracts-story smoke)
- `crates/krometrail-cdp/tests/support/chrome.rs` (new)
- `crates/krometrail-cdp/tests/support/static_fixture.rs` (new)
- `crates/krometrail-cdp/tests/target_reducer.rs` (new)
- `crates/krometrail-cdp/tests/session_supervision.rs` (new)
- `crates/krometrail-cdp/tests/chrome_session_real.rs` (new)
- `docs/ARCHITECTURE.md`

`crates/krometrail-cdp/src/lib.rs` is intentionally a third serialized edit to export session/targets; no story sharing it can run concurrently. Likewise, this story's dependency makes its `src/app.rs` and `tests/rust-runtime-smoke.rs` edits explicitly follow the contracts story's atomic transition; those files are shared sequentially, not concurrently.

## Doctor smoke contract

Broaden the contracts story's stable no-browser doctor smoke in `tests/rust-runtime-smoke.rs`; do not assume the old provisional test still exists. The resulting smoke accepts exactly two environment-dependent production outcomes: success with a stable `browser available:` summary for one or more discovered installations, or exit 1 with the same stable `browser_not_found` and recovery text when discovery is empty. It continues to reject provisional `unsupported`/`browser transport is not available` text. An `src/app.rs` unit fake records one `installations()` call and makes `connect()` panic, proving doctor performs discovery only and does not launch, attach, allocate a port, or acquire a profile.

## Acceptance criteria

- [x] Reducer tests directly construct the parent-defined supervisor/input/effect/target/reconnect types; subscription-before-enable plus snapshot reconciliation is idempotent across creation/attach/info/detach/destroy races, and only recordable page targets are published.
- [x] The complete `Suspended` transition table is exercised for every legal restoration and every illegal edge. Pre-suspension lifecycle is retained, exact browser target keys preserve `TargetId`, attachment generations reject stale events, and changed/missing keys close/create rather than URL/title-match.
- [x] Target-local failures leave unrelated targets/session alive. Transport loss enters finite reconnect; managed process termination instead emits `browser_process_terminated` and never reconnects or relaunches. Exhaustion/cancellation ends with its own typed error and bounded cleanup.
- [x] Initial visibility is probed and later visibility signals update the reducer without starting a screencast. Observable slow-subscriber revision lag cannot backpressure supervisor state; no unmeasurable upstream-lag assertion exists.
- [x] Managed stop closes/terminates only the owned browser and cleans profile correctly; attach stop detaches and leaves the external browser/Electron app alive.
- [x] Real Chrome against `tests/fixtures/browser/cdp-transport-gate` proves managed launch, attach-without-close, two isolated targets, disconnect/rebuild, and no process/profile leak. Electron has mandatory deterministic capability coverage plus an opt-in real endpoint test.
- [x] Root uses only the default-feature production connector; the contracts-story `UnavailableBrowserConnector` composition is removed rather than retained as fallback, and connector installation discovery delegates to `ChromeLauncher`. Doctor preserves stable empty-discovery `browser_not_found` behavior and satisfies the broadened smoke contract above.
- [x] Supervisor tracing covers session state/reconnect and target discovered/attached/changed/suspended/closed/failed with the parent's privacy fields. Transport compatibility and launcher discovery/launch/shutdown tracing are verified in stories 2 and 3, not deferred here.
- [x] Workspace format/check/test/clippy, real-browser command, default/no-default production feature checks, spike regression, and dependency scans pass; architecture reflects the final5 production boundary.
- [x] No production screencast ingestion lands.

## Implementation notes

- Added the transport-neutral target reducer with exact-key identity, pre-suspension restoration, connection-generation guards, bounded revisioned subscriber fan-out, target-local failures, visibility probing, finite reconnect, cancellation, and ownership-aware shutdown.
- Added the production connector composition over `ChromeLauncher` and cdpkit, preserving managed launch/profile guards until setup succeeds. Attached sessions detach without issuing `Browser.close`; managed sessions close and terminate only their owned process tree.
- Replaced the transitional root connector, made `doctor` discovery-only with stable success/no-browser outcomes, removed discovery probe filesystem mutation, and rolled `docs/ARCHITECTURE.md` forward to the final5 boundary. The compatibility probe tries exact page keys and never starts a screencast.
- Added deterministic reducer/supervision/fixture coverage, bounded real-Chrome managed launch and attach target coverage, and opt-in Electron/attached endpoint tests. Real transport reconnection is covered by the deterministic factory; no production screencast path was added.
- Verification completed: `cargo fmt --all --check`; workspace default and `--no-default-features` check/test; workspace clippy with `-D warnings`; cdpkit spike regression; dependency-boundary grep; `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test chrome_session_real -- --nocapture` (Chrome available; Electron and external attach opt-ins skipped when unset).
- Review fix: real-browser tests now own unique temporary roots with an RAII guard declared before browser/session guards. Drop prunes only empty known roots after checking Linux process command lines, preserving non-empty or actively referenced roots; startup cleanup handles stale empty roots and deterministic support coverage verifies the safety rule.
