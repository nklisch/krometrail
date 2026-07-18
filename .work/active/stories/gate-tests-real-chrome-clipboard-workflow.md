---
id: gate-tests-real-chrome-clipboard-workflow
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

## Acceptance

- The qualification is opt-in, real-browser locked, temporary-profile scoped, and uses the loopback secure interaction fixture with foreground focus policy.
- A permitted write/read round trip returns the exact sentinel; platform-owned `unsupported` or `interaction_failed` denial is an honest passing outcome with stable recovery.
- Successful write evidence contains byte count only, and status, bounded session events, operation result/debug output, denial diagnostics, and persisted parameters do not contain the sentinel.
- Existing scripted coverage remains the authority proving no permission grant or page activation command is sent.

## Tests

Run the named test with and without `KROMETRAIL_REAL_CHROME_TESTS=1`; the default path skips explicitly, while supported local Chrome executes the real workflow.

## Implementation

Added the opt-in managed-Chrome qualification with a temporary profile, foreground fixture, exact-success branch, stable platform-denial branch, evidence inspection, and bounded event inspection. The local Chrome run reached the isolated bridge and exercised the declared `interaction_failed` timeout/denial outcome without exposing the sentinel.

## Review

Bounded review confirmed the test excludes only the explicitly requested successful clipboard read from its leak scan, retains exact-value validation for that read, requires recovery on platform denial, and does not grant permissions or activate the page. Scripted clipboard tests remain the deterministic authority for command ordering and fixed-function/value-argument separation.
