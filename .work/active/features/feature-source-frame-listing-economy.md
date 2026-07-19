---
id: feature-source-frame-listing-economy
kind: feature
stage: review
tags: [agent-ux, temporal]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Source-frame listing economy

## Brief

`list_source_frames` concise pages are too large for an MCP host's per-result
token cap. One 64-frame page (the max the runtime allows) serialized to ~88 KB
/ >40k tokens and was rejected by the host before the agent could read it; each
frame row carries ~790 bytes (content_sha256, encoded_byte_len, media_type,
full provenance block with session/target ids, capture ordinal,
observed/source/session times, viewport, warnings, request/resolved positions,
scope). Pagination itself is correct (next_offset present until the tail,
absent on the last page — verified against a 361-frame range). The defect is
purely payload weight: the listing exists to let an agent discover frame ids to
drive `fetch_source_frames` / `generate_region_filmstrip`, and today the only
way through is offline jq/python over a saved oversized result file.

Fix direction from the shakedown: make the concise listing projection genuinely
concise — frame_id + resolved position + session time + byte length is enough
to drive follow-up fetches — and keep the full provenance behind
`detail: expanded`/`full` (and `fetch_source_frames`, which already returns it
per frame). Follows the project's canonical-result-projection pattern: the
projection changes presentation weight only, never outcomes or drill-down
authority.

## Simplification opportunity

The concise row currently duplicates the entire per-frame provenance that
`fetch_source_frames` already serves; trimming it removes redundant projection
code rather than adding a new surface. No new schema variants — the existing
`detail` dial is the selector.

Origin: `.work/backlog/idea-source-frame-listing-token-overflow.md` (2026-07-19
third shakedown).

## Architectural choice

Projection-only change in `krometrail-mcp` (canonical-result-projection): the
`ListSourceFrames` branch of the response projector gains a compact per-frame
row used when `detail: concise` (the default), exactly parallel to the existing
`compact_resolved_range` treatment of the range on the same branch. Expanded and
full detail keep serializing the complete `SourceFrameHandle` rows. No domain,
store, or wire-request changes; outcomes and drill-down authority (frame ids →
`fetch_source_frames`) are preserved. Alternatives rejected: lowering the page
size cap (punishes expanded callers and still leaves ~1.4KB/row), and a new
listing tool variant (violates one-registry/no-parallel-surface discipline).

## Implementation Units

### Unit 1: Compact source-frame rows
**File**: `crates/krometrail-mcp/src/response.rs`

```rust
#[derive(Serialize)]
struct CompactSourceFrameRow {
    frame_id: FrameId,
    resolved_position: u32,
    session_time: SessionTime,
    media_type: NonEmptyText,   // from handle.media_type (clone)
    encoded_byte_len: u64,
    #[serde(skip_serializing_if = "is_zero")]
    warning_count: u32,          // handle.provenance.warnings().len()
}
```

In the `ProgressiveEvidenceResult::ListSourceFrames` arm: when
`response.detail == ResponseDetail::Concise`, project `list.frames` through
`CompactSourceFrameRow` (session_time from `handle.provenance.session_time()`);
otherwise keep the current full serialization. Resource links
(`add_source_frame_resource`) are emitted in both cases, unchanged — they are
the drill-down authority.

**Implementation Notes**:
- Follow the exact style of `CompactResolvedRange`/`compact_resolved_range`.
- `warning_count` keeps bounded-loss visibility without shipping warning bodies.

**Acceptance Criteria**:
- [x] Concise 64-row listing serializes to well under a typical host token cap:
      assert a 64-frame concise projection body stays < 16 KB.
- [x] Concise rows carry frame_id, resolved_position, session_time, media_type,
      encoded_byte_len and nothing else (no provenance object, no sha256, no
      request_position).
- [x] `detail: expanded` still returns full `SourceFrameHandle` rows byte-for-byte
      as today.
- [x] Resource links still list every returned frame at all detail levels.

### Unit 2: Tool guidance
**File**: `crates/krometrail-mcp/src/registry.rs` (or wherever
`list_source_frames` description text lives)

Adjust the tool description if it promises full provenance at concise detail;
mention `detail: expanded` for full rows. Regenerate schemas/docs only if a
description string is part of checked-in canonical artifacts
(`canonical-json-schema-artifacts`).

## Implementation Order
1. Unit 1 (projection + tests)
2. Unit 2 (guidance)

## Testing
- Interface test in `response.rs` tests: concise listing row shape + size bound
  (protects the host token budget regression this feature exists for).
- Regression: expanded detail unchanged (protects drill-down contract).
- Remove nothing; no existing test asserts the oversized concise shape.

## Risks
- Downstream consumers of the concise listing shape: none supported (agent tool
  without third-party integrations; Current Contract Discipline applies).

## Implementation notes
- Execution capability: host implementation, because the projection and one registry description are a small cohesive write set.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-core/src/progressive.rs`.
- Tests added/removed: added a 64-row concise-size/shape/resource-link regression and expanded-row preservation coverage in `response.rs`.
- Simplification: removed the concise listing's redundant full-handle serialization; no compatibility path was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `CARGO_TARGET_DIR=/tmp/krometrail-target cargo test -p krometrail-mcp concise_source_frame_listing_is_small_and_keeps_only_drilldown_fields --locked` passed.
