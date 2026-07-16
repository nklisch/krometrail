---
id: gate-cruft-refresh-debug-bundle-module-comments
kind: story
stage: implementing
tags: [cleanup, documentation, visual]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: cruft
created: 2026-07-15
updated: 2026-07-15
---

# Refresh stale temporal debug-bundle module comments

## Confidence
Medium

## Category
Stale comments

## Location
`src/debug_bundle/mod.rs:6`

## Evidence

The module comments describe `TemporalDebugBundleService` as upcoming and not wired, while `src/app.rs` now constructs and composes it in the production runtime.

## Removal

Replace pre-integration scaffolding language with a concise description of the currently wired service and its pure policy/marker/focus helpers. Change no runtime behavior or future foundation intent.
