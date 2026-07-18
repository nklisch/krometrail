---
id: gate-tests-wait-for-page-operation-loop
kind: story
stage: done
tags: [testing]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: tests
created: 2026-07-18
updated: 2026-07-18
---

# Protect the wait-for-page operation loop

## Priority
High

## Value evidence

Item: `epic-agent-browser-ergonomics-browser-contexts-page-relationships`

The public contract promises post-cursor popup reconciliation, opener filtering, monotonic ordering, timeout, caller cancellation, shutdown/disconnect cancellation, and no focus activation. Existing reducer tests do not invoke the production polling/reconciliation loop in `crates/krometrail-cdp/src/session/operations.rs:186`.

## Gap type
important-interface

## Suggested test

Exercise the actual loop with scripted target inventories: capture a cursor, expose a popup with the expected opener, assert a later sequence and no activation commands, then cover caller cancellation, timeout, and browser disconnect terminal outcomes.

## Test location
`crates/krometrail-cdp/src/session/operations.rs`

## Acceptance

- A deterministic test invokes the production browser-session operation loop, reconciles a post-cursor popup with the requested opener, and proves no activation command is emitted.
- Timeout and caller cancellation return their stable codes, while session shutdown cancellation and transport disconnect terminate promptly with `cancelled` and `browser_disconnected` respectively.
- The operation test does not use wall-clock sleeps for orchestration; scripted command observation and bounded timeouts provide readiness and liveness fences.

## Test notes

Use the existing scripted CDP transport through `ProductionBrowserConnector` so polling, reducer reconciliation, effect application, and the public operation dispatcher are all exercised.

## Implementation notes

- Execution capability: focused inline deterministic integration coverage over the existing production connector and scripted CDP seam.
- Review weight: bounded standalone-story review, per gate-bundle caller.
- Files changed: session operation loop and `waits_and_batches` integration tests.
- Tests added: post-cursor opener-filtered popup reconciliation with no activation; timeout; caller cancellation; session-stop cancellation; transport disconnect.
- Simplification: no new harness; session-stop cancellation now uses the existing combined cancellation authority rather than checking only the caller signal.
- Discrepancies from design: integration coverage lives in `tests/waits_and_batches.rs` so it can reuse the production-session scripted transport; it directly invokes the operation loop named by the design.
- Adjacent issues parked: none.

## Bounded inline review — 2026-07-18

- Verdict: approved. Tests cross the public session dispatcher, production poll/reconcile/effect loop, and exact cancellation/disconnect outcomes; command history proves focus is untouched.
