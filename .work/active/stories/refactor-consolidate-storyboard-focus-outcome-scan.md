---
id: refactor-consolidate-storyboard-focus-outcome-scan
kind: story
stage: implementing
tags: [refactor, visual]
parent: null
depends_on: [epic-temporal-debugging-workflow-temporal-debug-bundle]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Consolidate storyboard focus-outcome scanning

## Brief

`src/debug_bundle/focus.rs:84-119` and `src/debug_bundle/focus.rs:127-162`
repeat the same `ArtifactOutcome::Available` → storyboard-kind →
`storyboard_selection()` filtering before reading different candidate ranks.
The first loop inserts summary moments and the second loop inserts selected
frames with major-change reasons. Because candidates are stored in a
`BTreeSet<FocusCandidate>` ordered by the explicit rank key, both passes can be
performed in one outcome traversal without changing the final set.

Use one local traversal to establish the storyboard selection once per outcome,
then add the summary candidates and major-reason frame candidates from that
selection. Keep the rank constants, `BTreeSet` ordering, deduplication, cap, and
chronological output unchanged.

**Source lens**: missing abstraction / exact repeated validation and projection

**Rationale**: removes one duplicated outcome/manifest filter and makes the
single typed storyboard-trace authority easier to audit, without adding a
second selector or changing visual evidence semantics.

**Black-box classification**: pure refactor. Identical artifact outcomes
produce identical focus times, including epoch/frame/ID tie breaks, rank
priority, 16-item capping, chronological sorting, unchanged traces, and gaps.

## Current State

The function performs two independent passes over `outcomes`, each repeating:

```rust
let ArtifactOutcome::Available { epoch_index, artifact, .. } = outcome else { continue };
if artifact.manifest.artifact_kind() != ArtifactKind::Storyboard { continue; }
let Some(selection) = artifact.manifest.storyboard_selection() else { continue };
```

The duplicated gate is private implementation code introduced with the typed
manifest trace; no second measurement or selection call occurs today.

## Target State

One pass obtains each available storyboard selection once and contributes both
candidate families. The `BTreeSet` remains the authority for policy-rank and
stable tie ordering, and the final `seen_times` deduplication, cap, and
chronological projection remain unchanged.

## Acceptance Criteria

- [ ] `extract_focus_times` has one outcome/manifest eligibility traversal; no duplicate storyboard-selection gate remains.
- [ ] Summary candidates retain ranks 0–2 and selected major-change frames retain rank 3, with the same key ordering and cap behavior.
- [ ] Existing changed, unchanged, unavailable, multi-epoch tie, duplicate-time, gap, and cap tests pass unchanged; no measurement/selection API is called by the extractor.
- [ ] No manifest, header, context-request, serialized, or MCP response behavior changes.
- [ ] `cargo fmt --all -- --check`, locked workspace check/test, and Clippy with `-D warnings` pass.

## Risk and Rollback

**Risk**: Low. `BTreeSet` ordering makes insertion-pass order irrelevant, but an
accidental rank/key change could alter capped results.

**Rollback**: Revert the implementation commit to restore the two explicit
passes. No artifact, manifest, cache, or compatibility rollback is needed.

## Discovery Notes

- **Scope**: the completed temporal bundle and adjacent temporal-vision trace
  path in commits `6b5776b` through `245fb1f`; verified directly in
  `src/debug_bundle/focus.rs` and its focused tests.
- **Dispatch**: direct-read only; no exploratory agent or peer review was used.
- **Project conventions**: no project refactor-convention catalog exists; the
  built-in duplication and single-source-of-truth lenses were applied.
- `.work/bin/work-view` and current epic/feature stages were not changed.
