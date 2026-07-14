---
id: refactor-use-typed-artifact-kind-ranking
kind: story
stage: implementing
tags: [refactor, agent-ux, visual]
parent: null
depends_on: [epic-temporal-debugging-workflow-mcp-investigation-surface]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Derive MCP artifact priority from the typed kind

## Brief

The temporal primary-image projector calls
`artifact.manifest.artifact_kind().as_str()` and then re-matches the stable
serialized names in `crates/krometrail-mcp/src/response.rs:606-608,876-884`.
That duplicates the `temporal_vision::ArtifactKind` registry's stable-name
mapping at the MCP boundary. A future artifact-kind name change could leave
primary-image selection silently defaulting to the fallback rank.

Make the private ranking helper accept `temporal_vision::ArtifactKind` and
match the typed variants directly. Preserve the current preference order:
`BeforeDuringAfter`, `Storyboard`, `DifferenceMap`, then all other kinds.

**Source lens**: pattern drift / split variant authority

**Rationale**: keeps the MCP presentation policy attached to the authoritative
artifact-kind enum instead of copying its serialized strings into a second
match, while preserving the exact current selection order and fallback.

**Black-box classification**: pure refactor. Current artifact outcomes select
the same primary artifact by `(kind rank, epoch, generator, artifact id)` and
all resource/image response contracts remain unchanged.

## Current State

```rust
let rank = artifact_kind_rank(artifact.manifest.artifact_kind().as_str());

fn artifact_kind_rank(kind: &str) -> u8 {
    match kind {
        "before_during_after" => 0,
        "storyboard" => 1,
        "difference_map" => 2,
        _ => 3,
    }
}
```

The string literals are a second authority for names already owned by
`temporal_vision::ArtifactKind`.

## Target State

```rust
let rank = artifact_kind_rank(artifact.manifest.artifact_kind());

fn artifact_kind_rank(kind: temporal_vision::ArtifactKind) -> u8 {
    match kind {
        temporal_vision::ArtifactKind::BeforeDuringAfter => 0,
        temporal_vision::ArtifactKind::Storyboard => 1,
        temporal_vision::ArtifactKind::DifferenceMap => 2,
        _ => 3,
    }
}
```

Use the existing typed enum; do not add a new registry, ranking abstraction,
or public API.

## Acceptance Criteria

- [ ] The primary-image projector no longer converts `ArtifactKind` to a stable-name string for ranking.
- [ ] Preference order, fallback exclusion, epoch/generator/ID tie ordering, inline limits, resource links, and response metadata remain unchanged.
- [ ] Existing temporal response and resource tests pass; add no test solely for a private enum match unless existing coverage exposes a gap.
- [ ] No artifact registry, descriptor, serialized name, cache identity, or MCP schema changes.
- [ ] `cargo fmt --all -- --check`, locked workspace check/test, and Clippy with `-D warnings` pass.

## Risk and Rollback

**Risk**: Low. The helper is private and the current enum variants map one to
one to the existing string cases; the main risk is accidentally changing the
fallback rank.

**Rollback**: Revert the implementation commit to restore the string-based
helper. No resource, cache, or compatibility rollback is required.

## Discovery Notes

- **Scope**: temporal MCP response/resource projection and adjacent artifact
  contracts landed in commits `6b5776b` through `245fb1f`; verified directly in
  `crates/krometrail-mcp/src/response.rs` and temporal-vision's registry-backed
  provenance types.
- **Dispatch**: direct-read only; no exploratory agent or peer review was used.
- **Project conventions**: no `.agents/skills/refactor-conventions/` catalog is
  present. This finding uses the project-wide single-source-of-truth principle.
- `.work/bin/work-view` and current epic/feature stages were preserved.
