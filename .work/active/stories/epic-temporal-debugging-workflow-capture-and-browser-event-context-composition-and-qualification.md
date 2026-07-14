---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-composition-and-qualification
kind: story
stage: done
tags: [browser, storage, agent-ux, security, testing]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on:
  - epic-temporal-debugging-workflow-capture-and-browser-event-context-session-domain-authority-and-routing
  - epic-temporal-debugging-workflow-capture-and-browser-event-context-range-context-query-service
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Compose and Qualify Browser Event Context

## Checkpoint

Wire one concrete `RecordingStore` as browser-event sink/source and temporal-context service, enable browser-events from the existing default capability registry, retain explicit disable/degraded semantics, and qualify the complete fake-CDP-to-sanitized-range-context path. MCP, bundles, artifacts, and resources remain unchanged.

## Files

- `src/app.rs`
- `crates/krometrail-cdp/src/session/{mod.rs,runtime.rs,shutdown.rs}`
- `crates/krometrail-cdp/tests/browser_events.rs`
- `crates/krometrail-store/tests/{browser_events.rs,range_context.rs,retention_small_budget.rs}`
- existing root composition tests in `src/app.rs`

## Acceptance evidence

- Root defaults to operational `browser-events` with the same clock/IDs/session origin/store as capture; explicit disable adds no semantic subscriptions and leaves control/capture/network waits intact.
- Two-target/two-generation/reconnect routing reaches the v5 store and same-range query with exact identity/timing and source-safe status/logs.
- Serialized-row/error redaction corpus excludes fill/dialog/upload values, URL credentials/query/fragments, console/exception secrets, stack/local paths, forbidden network fields, headers/cookies/auth, and body sentinels.
- Network waits coexist under event saturation and fail on fan-out lag instead of claiming quiet.
- v4→v5, usage/independent event retention/pins/session deletion/recovery/corruption and query ordering/focus/truncation/capture-quality cases all pass.
- Controlled barriers prove no event flood or store stall starves frame acknowledgement/ingestion, target supervision, operations, or another target.
- Rust 1.85 locked format, workspace all-target check/test, and Clippy `-D warnings` pass; no live-Chrome claim is made unless its ignored/manual test is enabled.

## Implementation

- Root composition now creates one default capability selection and derives `BrowserEventConfig` from its `BrowserEvents` membership. The same selection reaches MCP without adding routes, and explicit omission installs disabled collection while leaving the independently composed control and capture services present.
- The production connector receives the same process clock and ID source as capture plus a browser-event sink view of the one concrete `RecordingStore`. That store is also retained as the `TemporalContextQuery`; root composition tests prove the sink, source, and context trait objects share the concrete allocation.
- The scripted adapter qualification now covers explicit disabled subscription/enable behavior and a two-target reconnect across two transport generations into a real schema-v5 `RecordingStore`. It verifies stable Krometrail session/target identity, generation fencing, continuing event ordinals, exact stored-to-context event identity/time/order, and a same-range two-frame capture-quality result.
- The integrated private corpus injects ignored fill/upload/body/header/cookie/auth fields, credential/query/fragment URLs, exception/console/stack secrets and local paths, raw request IDs, and dialog/default/input values. Persisted decoded rows contain none of the sentinels. Existing raw-row corruption tests continue to inspect the SQLite payload itself and source-safe errors.
- Existing bounded barrier coverage remains authoritative rather than duplicated: capture acknowledges before handoff and drops after ack, event writers isolate a stalled target from another target, operation/supervisor deadlines remain independent, network fan-out reports lag instead of quiet, and aggregate shutdown aborts writers at the shared deadline. The full workspace run also qualified v4-to-v5 migration/rollback, usage, independent event eviction, pins, deletion, recovery/idempotence/corruption, and all range-context cases.

## Verification

Rust `1.85.0` (`rustc 1.85.0 (4d91de4e4 2025-02-17)`), all locked:

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed, including 106 CDP unit tests, 8 browser-event integration tests, 27 store unit tests, 3 browser-event store tests, 2 browser-event recovery tests, 10 range-context tests, 10 small-budget retention tests, and the remaining workspace targets.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.

No live-Chrome execution is claimed: opt-in/manual tests retained their skip behavior when no configured browser was supplied. No CDP runtime, shutdown, store schema, MCP route/resource, artifact, temporal-vision, or foundation-document change was needed for this checkpoint.

## Ordering

Depends on both the CDP domain authority and the durable range-context query. It closes the feature as one cohesive implementation/review bundle.