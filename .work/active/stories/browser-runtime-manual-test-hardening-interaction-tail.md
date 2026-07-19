---
id: browser-runtime-manual-test-hardening-interaction-tail
kind: story
stage: done
tags: [browser, visual, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Clamp eligible natural interaction tails to captured bounds

Under `AllowPartial`, intersect only interaction-derived natural ranges with retained evidence while preserving requested provenance, limitations, and exact behavior for explicit or require-complete requests.

## Implementation

- Retention classification delegates out-of-capture-bound interaction and latest-interaction ranges to one eligibility helper. Only `AllowPartial` natural interaction ranges with a nonempty retained intersection can clamp.
- The resolved interval is the exact requested/retained intersection. The original requested interval and interaction reference remain unchanged, and `PartiallyCaptured` plus affected-edge warnings make the capture-bound limitation explicit.
- `RequireComplete`, explicit session-time intervals, and wholly disjoint natural interactions retain exact `not_found` behavior. Existing eviction-hole handling remains independent and fail-closed.

## Verification

- `cargo fmt --all -- --check`
- `cargo test -p krometrail-core timeline::range::tests --locked` (7 passed)
- `cargo test -p krometrail-store --test range_resolution --locked` (8 passed)

The first focused fixture run correctly rejected a read-only `SnapshotPage` operation as an interaction anchor. The fixture was corrected to use the state-changing `NavigatePage` operation and the complete focused suite then passed.

## Tooling deviation

`.work/bin/work-view` is a Linux executable and cannot run on this macOS host. The item and dependency state were inspected directly from the `.work/` Markdown substrate.
