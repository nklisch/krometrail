# Narrowest Temporal Scope

Validate, filter, clip, or materialize time-bearing evidence against the narrowest scope consumed downstream: requested range, retained range, then visual epoch.

## Rationale

Temporal values can be valid for a session yet invalid for a requested interval, retained frame set, or visual epoch. Narrowing at each boundary prevents valid semantic provenance from becoming an impossible artifact coordinate while preserving the original request for explanation and audit.

## Examples

- `crates/krometrail-core/src/artifacts.rs:410` — explicit artifact markers outside the requested range are rejected at the public boundary.
- `src/debug_bundle/markers.rs:137` — automatically derived markers are filtered to the resolved range while navigation and generic markers retain their intended semantics.
- `src/artifacts/epoch.rs:286` — markers are clipped again when evidence is partitioned into visual epochs.
- `src/artifacts/generators.rs:79` — a storyboard render anchor is clamped to the retained source-frame interval for its epoch without replacing semantic range provenance.

## When to Use

Use whenever session-relative timestamps, anchors, markers, frames, or events pass from a broad temporal domain into a narrower query, retention, epoch, or artifact domain.

## When Not to Use

Do not overwrite the original semantic request or provenance merely to fit available evidence. Keep source, observed, normalized, and render-local time distinct.

## Common Violations

- Accepting a session-valid marker that lies outside the resolved request range.
- Reusing one query anchor for every visual epoch without checking retained frame bounds.
- Silently changing semantic provenance to match a render-local clamp.
- Applying containment only to caller input while trusting automatically derived evidence.
- Treating an absent frame interval as an invitation to invent a timestamp.
