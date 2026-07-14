---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-session-domain-authority-and-routing
kind: story
stage: implementing
tags: [browser, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: [epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Build Session Domain Authority and Event Routing

## Checkpoint

Give each supervised session one owner for Runtime/Log/Network/Page event subscriptions and domain enablement. Subscribe before ordered restore, continuously drain cdpkit named streams into bounded nonblocking normalization/persistence queues, fence targets by attachment/connection generation, coalesce drops, and fan one Network activity stream to recording and explicit network-quiet waits without duplicate enable/disable or event stealing.

## Files

- `crates/krometrail-cdp/src/events/{mod.rs,domain.rs,normalize.rs,privacy.rs,network.rs,pipeline.rs,status.rs}` (new)
- `crates/krometrail-cdp/src/{compatibility.rs,lib.rs}`
- `crates/krometrail-cdp/src/session/{mod.rs,runtime.rs,reconnect.rs,shutdown.rs}`
- `crates/krometrail-cdp/src/control/wait.rs`
- `crates/krometrail-cdp/src/capture/{mod.rs,pipeline.rs}`
- `crates/krometrail-cdp/tests/{browser_events.rs,waits_and_batches.rs,session_supervision.rs}`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`

## Acceptance evidence

- Same-named events route across two targets and reconnect generations without crossing scope; late old-generation callbacks cannot allocate or persist.
- Semantic streams install once before exact Page/lifecycle/Runtime/Log/Network/Accessibility restore order; optional event failures degrade without failing mandatory control/capture.
- Network-quiet subscribes to the shared bounded fan-out before on-demand enable, excludes long-lived connections, and fails explicitly on lag.
- Defaults enforce 256 queued events/target, 16 MiB global pending payload, 128-row/256-KiB batches, 1,024 Network fan-out, 4,096 request correlations, and 64 gap entries.
- Redirect/out-of-order network events, source clocks, dialogs, target lifecycle, capture status, and visibility normalize through allowlisted fields only.
- Saturated/failing event persistence records bounded drop/degradation evidence and cannot delay frame ack/handoff, supervisor reconnect, operations, or another target.

## Ordering

Depends only on core event/privacy contracts. This checkpoint deliberately remains implementable while artifact schema v4 and event schema v5 are pending.