---
id: gate-tests-real-chrome-clipboard-workflow
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

# Qualify the managed clipboard workflow in real Chrome

## Priority
High

## Value evidence

Item: `epic-agent-browser-ergonomics-local-io-clipboard`

The public clipboard workflow currently has scripted CDP tests only. Its focus/permission/platform behavior and sentinel privacy need one actual managed-Chrome qualification that accepts the declared platform-owned denial contract where permission cannot be granted.

## Gap type
important-interface

## Suggested test

Launch the secure focused fixture, explicitly write/read a sentinel when Chrome permits it or assert the stable unsupported/interaction-failed recovery contract, assert no permission/focus mutation command, and prove the sentinel is absent from status, events, diagnostics, tracing, and persisted interaction parameters.

## Test location
`crates/krometrail-cdp/tests/verified_interactions.rs`
