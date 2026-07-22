---
id: feature-temporal-anchor-ergonomics-latest-interaction-bundle
kind: story
stage: done
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

## Implementation

Root cause matched the hypothesis exactly. The resolver collapses
`latest_interaction` to an `interaction` anchor at
`crates/krometrail-core/src/timeline/range.rs` (the `LatestInteraction` arm calls
`seed_from_interaction(..., TemporalRangeAnchorKind::Interaction)`), and a
`ResolvedRange` can never carry `LatestInteraction`: `validate_anchor_kind`
rejects it and the wire enum `ResolvedRangeAnchorKindWire` excludes it by
design, with a doc comment documenting the collapse. The bundle's
post-resolution invariant `validate_query_resolution` in
`crates/krometrail-core/src/debug_bundle.rs` compared the raw request anchor
kind (`latest_interaction`) against the collapsed resolved kind
(`interaction`), tripping "resolved range must preserve the exact temporal
query options". The anchor-identity arm of the same function already tolerated
the collapse (`LatestInteraction` request vs `Interaction` reference), so only
the kind-equality clause was broken. `resolve_temporal_range` never runs this
cross-check, which is why it accepted the identical anchor.

Fix: added `TemporalRangeAnchorKind::resolved_kind()` in
`crates/krometrail-core/src/timeline/range.rs` — the one domain statement of
the collapse (`LatestInteraction` maps to `Interaction`; every other kind maps
to itself) — and made `validate_query_resolution` compare
`request.anchor.kind().resolved_kind()` against `range.anchor_kind`. No wire
schema changes; `ResolvedRangeAnchorKind` already excluded `latest_interaction`
and the request schema is untouched (`check-wire-enum-schemas.sh` green).

Files changed:
- `crates/krometrail-core/src/timeline/range.rs` — `resolved_kind()` plus a
  registry test asserting the mapping for every anchor kind.
- `crates/krometrail-core/src/debug_bundle.rs` — invariant compares the
  collapsed kind.
- `src/debug_bundle/tests.rs` — regression test
  `latest_interaction_anchor_resolves_through_bundle_service` in the
  qualification module: bundles via `latest_interaction` against the real
  store rig and asserts the collapsed `interaction` resolved kind, exact
  interaction identity, and the mandatory anchor marker. Verified to fail with
  the pre-fix invariant and pass with the fix.

Full gate green: fmt, wire-enum schema check, check, test (workspace, all
targets), clippy `-D warnings`.
