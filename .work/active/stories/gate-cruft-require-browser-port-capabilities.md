---
id: gate-cruft-require-browser-port-capabilities
kind: story
stage: drafting
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Require new browser-port capabilities explicitly

## Confidence
Medium

## Category
compatibility shim

## Location
`crates/krometrail-core/src/ports/browser.rs:544`

## Evidence

New managed-profile and managed-download capabilities provide empty/not-found trait defaults, allowing adapters and test doubles to omit the capabilities silently even though workspace crates are unpublished internals.

## Removal

Make the new trait methods required and add explicit empty/not-found behavior only to adapters or fakes where it is intentional, restoring compiler-enforced adapter completeness.
