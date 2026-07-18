---
id: gate-tests-real-chrome-browser-context-seam
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

# Qualify frame actions and page assets in real Chrome

## Priority
High

## Value evidence

Item: `epic-agent-browser-ergonomics-browser-contexts`

Owner-frame geometry, frame-scoped query/action, stale navigation fencing, and privacy-bounded Resource Timing form the riskiest browser-protocol seam. Existing coverage uses constructed frame trees and parsed asset rows but does not drive `list_frames` → frame `query_page` → referenced action or `list_page_assets` against Chrome.

## Gap type
e2e-seam

## Suggested test

Add one opt-in real-Chrome fixture that scrolls the root, queries/clicks a same-origin child-frame target, proves the reference stale after child navigation, and checks bounded sanitized asset metadata without raw query/fragment/path/content leakage.

## Test location
`crates/krometrail-cdp/tests/verified_interactions.rs`
