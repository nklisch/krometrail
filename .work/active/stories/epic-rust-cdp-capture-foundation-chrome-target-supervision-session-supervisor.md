---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter, epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Supervise target sessions, reconnect, and shutdown

## Scope

Implement Unit 4 of the parent design: one deterministic target reducer, flat discovery/auto-attach reconciliation, session event publication, target-local failure isolation, finite reconnect/rebuild, explicit cancellation and ownership-correct shutdown, root connector wiring, truthful discovery-only `doctor`, architecture roll-forward, and real-browser integration evidence.

Consume the completed contract, transport, and launcher stories. Do not implement production screencast start/events/acknowledgement, frame queues, persistence, capture gaps, browser actions, or snapshots.

## Required files

- `crates/krometrail-cdp/src/targets/{mod.rs,model.rs,reducer.rs,supervisor.rs}`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs`, `src/cli.rs`, `tests/rust-runtime-smoke.rs`
- `crates/krometrail-cdp/tests/support/{chrome.rs,static_fixture.rs}`
- `crates/krometrail-cdp/tests/{target_reducer.rs,session_supervision.rs,chrome_session_real.rs}`
- `docs/ARCHITECTURE.md`

## Acceptance criteria

- [ ] Subscription-before-enable plus snapshot reconciliation is idempotent across creation/attach/info/detach/destroy races; only recordable page targets are published.
- [ ] Exact browser target keys preserve `TargetId` across reconnect, attachment generations reject stale events, and changed/missing keys close/create rather than URL/title-match.
- [ ] Target-local failures leave unrelated targets/session alive; transport loss enters reconnecting, finite policy rebuilds complete state, and exhaustion/cancellation ends with typed errors and bounded cleanup.
- [ ] Initial visibility is probed and later visibility signals can update the reducer without starting a screencast. Slow observers cannot backpressure supervisor state.
- [ ] Managed stop closes/terminates only the owned browser and cleans profile correctly; attach stop detaches and leaves the external browser/Electron app alive.
- [ ] Real Chrome against `tests/fixtures/browser/cdp-transport-gate` proves managed launch, attach-without-close, two isolated targets, disconnect/rebuild, and no process/profile leak. Electron has mandatory deterministic capability coverage plus an opt-in real endpoint test.
- [ ] Root uses the production connector; `doctor` performs discovery only. Structured logs satisfy the parent's privacy requirements.
- [ ] Workspace format/check/test/clippy, real-browser command, spike regression, and dependency scans pass; architecture reflects the final5 production boundary.
- [ ] No production screencast ingestion lands.
