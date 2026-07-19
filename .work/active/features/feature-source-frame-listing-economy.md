---
id: feature-source-frame-listing-economy
kind: feature
stage: drafting
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
