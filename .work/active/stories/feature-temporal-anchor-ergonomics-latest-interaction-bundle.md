---
id: feature-temporal-anchor-ergonomics-latest-interaction-bundle
kind: story
stage: implementing
tags: [visual, agent-ux, bug]
parent: feature-temporal-anchor-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Fix `temporal_debug_bundle` for the `latest_interaction` anchor

`temporal_debug_bundle` fails with the `latest_interaction` anchor. Found during
the 2026-07-21 v1.4.0 MCP shakedown and independently reported in GitHub issue
#14 (finding #6) from a separate macOS E2E run — two independent reproductions.

## Repro

Calling `temporal_debug_bundle` with `query.anchor = {anchor: "latest_interaction",
session_id, target_id}` (with or without a `window`) returns:

> `temporal_debug_bundle failed: resolved range must preserve the exact temporal query options`

Contrast — both of these succeed:
- `resolve_temporal_range` with the **identical** `latest_interaction` anchor returns a valid
  `range_handle`.
- `temporal_debug_bundle` with an explicit `{anchor: "interaction", interaction_id}` anchor
  for the very same interaction returns a full bundle.

So the bundle path is the only broken one, and only for the collapsing `latest_interaction`
anchor.

## Likely cause

`latest_interaction` collapses to an `interaction` anchor during resolution — the wire
schema itself notes: "A resolved range carries the resolver-selected kind, not the
request-only `latest_interaction` anchor that has already collapsed to `interaction`." The
bundle then re-validates that the resolved range's options / anchor-kind exactly equal the
request's, which no longer matches after the collapse, tripping the invariant.
`resolve_temporal_range` already tolerates this.

## Impact

`latest_interaction` is the most ergonomic "just bundle the last thing that happened" entry
point into the temporal bundle, and it is currently unusable. Callers must first
`resolve_temporal_range` (or capture the `interaction_id` from the acting tool's response)
and pass an explicit anchor. Not a data-loss or privacy issue — purely an ergonomic dead end
on a documented anchor.

## Acceptance

- The bundle's post-resolution invariant check compares against the
  collapsed/resolved anchor kind rather than the request-only anchor, matching
  how `resolve_temporal_range` already handles the collapse.
- A regression test bundles via `latest_interaction` successfully.
