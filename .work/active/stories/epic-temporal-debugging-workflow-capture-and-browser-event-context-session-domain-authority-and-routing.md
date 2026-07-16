---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-session-domain-authority-and-routing
kind: story
stage: done
tags: [browser, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: [epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Build Session Domain Authority and Event Routing

## Checkpoint

Give each supervised session one owner for Runtime/Log/Network/Page event subscriptions and domain enablement. Subscribe before ordered restore, continuously drain cdpkit named streams into bounded nonblocking normalization/persistence queues, fence targets by attachment/connection generation, coalesce drops, and fan one Network activity stream to recording and explicit network-quiet waits without duplicate enable/disable or event stealing.

## Files

- `crates/krometrail-cdp/src/events/{mod.rs,domain.rs,normalize.rs,privacy.rs,network.rs,pipeline.rs,signals.rs,status.rs}` (new)
- `crates/krometrail-cdp/src/{compatibility.rs,lib.rs}`
- `crates/krometrail-cdp/src/session/{mod.rs,operations.rs,runtime.rs,reconnect.rs,shutdown.rs}`
- `crates/krometrail-cdp/src/control/{mod.rs,interaction.rs,wait.rs}`
- `crates/krometrail-cdp/tests/{browser_events.rs,page_lifecycle.rs,verified_interactions.rs,waits_and_batches.rs}`
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

## Implemented decisions

- `SessionDomainAuthority` is the sole supervised-session owner of semantic Runtime/Log/Network/Page subscriptions and Page/Runtime/Accessibility plus optional lifecycle/Log/Network enablement. It installs bounded drains before the exact restore sequence and exposes privacy-free lifecycle/dialog signals to interactions instead of allowing competing named subscriptions.
- One generation-bound runtime exists per attached target. The `(connection_generation, attachment_generation, transport_session)` binding fences callbacks before allocation; reconnect restores domains and visibility transactionally under the existing attempt deadline, then publishes the prepared state.
- Network events normalize once into a bounded activity value. Persistence receives the same normalized activity through nonblocking per-target ingress, while network-quiet receives a bounded broadcast subscription before on-demand installation/enablement. Repeated waits neither resubscribe nor re-enable, long-lived requests are excluded, and fan-out lag is explicit.
- Per-target writers isolate sink stalls. Global pending bytes, queue rows, batch rows/bytes, request correlations, fan-out, active targets, and gap ledgers all have validated hard caps. Ordinals are allocated before nonblocking enqueue, and failures become coalesced collection-gap evidence.
- Target retirement closes acceptance without waiting on persistence. Aggregate session shutdown owns event drain/flush under the same absolute deadline as capture and browser cleanup, and explicitly aborts a writer that exceeds it.
- Scripted transport fixtures now support session-scoped streams, live delivery, and a cross-command/subscription activity trace so routing and subscribe-before-enable order are asserted directly.

## Verification evidence

Rust 1.85 verification ran in an isolated detached worktree based on `622f9be` with only this checkpoint's patch applied:

- `cargo fmt --package krometrail-cdp -- --check` — passed.
- `cargo check -p krometrail-cdp --all-targets --locked` — passed.
- `cargo test -p krometrail-cdp --all-targets --locked` — passed; 106 unit tests plus every CDP integration target.
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — passed.

Focused evidence covers two-target and reconnect-generation routing, stale-generation non-allocation, exact subscription/restore order, bounded defaults and fan-out lag, redirect/out-of-order correlation and privacy, network-quiet sharing, cross-target non-starvation, rejected persistence gap evidence, and complete/deadline-exhausted event flush behavior.

No artifact schema, event schema v5, storage, context-query, composition, or parent-feature transition was included. `page_lifecycle.rs` and `verified_interactions.rs` only received expectation updates for the newly explicit optional compatibility/lifecycle setup commands.