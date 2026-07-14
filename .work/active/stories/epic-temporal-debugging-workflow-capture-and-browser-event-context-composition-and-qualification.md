---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-composition-and-qualification
kind: story
stage: implementing
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

## Ordering

Depends on both the CDP domain authority and the durable range-context query. It closes the feature as one cohesive implementation/review bundle.