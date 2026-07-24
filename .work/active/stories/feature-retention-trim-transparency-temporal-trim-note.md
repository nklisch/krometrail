---
id: feature-retention-trim-transparency-temporal-trim-note
kind: story
stage: done
tags: [store]
parent: feature-retention-trim-transparency
depends_on: [feature-retention-trim-transparency-status-transparency]
release_binding: 1.6.1
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Trim-aware temporal / artifact / query responses with a trimmed-through boundary

## Checkpoint

Unit 4 of the parent feature. Calmly note active in-session trimming (and any
grace override) on the response surfaces where range work happens, with a concrete
how-far-back reference. Design in the parent body (`## Architectural choice` →
Option B, `## Implementation Units` → Unit 4).

- `crates/krometrail-core/src/timeline/range.rs`: add
  `RetentionWarning::InSessionTrimmingActive { oldest_retained }` and
  `ArtifactGraceOverridden { oldest_retained }` — calm, factual variants reusing
  the existing warning shape.
- `crates/krometrail-mcp/src/registry.rs`: in `resolve_temporal_range`,
  `query_browser_events`, and `generate_artifacts` handlers, fetch the store
  `RetentionStatus` alongside the existing `capture_health` read and inject the
  variants into `capture_quality.retention_warnings` before projection (so the
  concise `retention_warning_count` stays consistent). `temporal_debug_bundle`
  inherits via the shared result shape.
- Omit both notes when the scope has no `oldest_retained` (empty store).

Depends on the status-transparency story for `trim_state` / `grace_override_active`;
does not depend on the grace-ordering story (reads the same latched status).

## Done when

- With the store `Trimming`, the three tools carry `InSessionTrimmingActive` with
  the oldest-retained session time; `Steady` carries none.
- After a grace override, the same responses carry `ArtifactGraceOverridden`.
- Empty store carries no note. Tone informational; may reference
  `pin_resolved_range` where relevant in plugin/docs text (not the enum).

## Implementation notes

- Added the two boundary-based `RetentionWarning` variants and injected them at
  the MCP handlers for temporal range resolution, browser-event context, and
  progressive artifact generation. Empty retained scopes omit the notes; the
  temporal debug bundle inherits the range warnings.
- Added `registry::tests::retention_notes_are_boundary_based_and_omitted_without_retained_evidence`.
- Verification: MCP/core focused tests and generated schema tests passed; the
  final locked workspace gate passed.
