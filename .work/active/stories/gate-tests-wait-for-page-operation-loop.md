---
id: gate-tests-wait-for-page-operation-loop
kind: story
stage: implementing
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
