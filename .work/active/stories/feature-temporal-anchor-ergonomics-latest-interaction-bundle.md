---
id: idea-bundle-latest-interaction-anchor
created: 2026-07-21
updated: 2026-07-21
tags: [temporal, bug]
---

`temporal_debug_bundle` fails with the `latest_interaction` anchor. Found during a fresh
v1.4.0 MCP shakedown.

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

## Fix direction

In the bundle's post-resolution invariant check, compare against the collapsed/resolved
anchor kind rather than the request-only anchor, matching how `resolve_temporal_range`
already handles the collapse. Add a regression test that bundles via `latest_interaction`.
