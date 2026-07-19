---
id: idea-source-frame-listing-token-overflow
created: 2026-07-19
updated: 2026-07-19
tags: [agent-ux, temporal]
---

Found in the 2026-07-19 third shakedown (final shipped build): `list_source_frames`
concise pages are too large for an MCP host's per-result token cap. One 64-frame
page (the max the runtime allows) serialized to ~88 KB / >40k tokens and was
rejected by the host before I could read it; each frame row is ~790 bytes
(content_sha256, encoded_byte_len, media_type, full provenance block with
session/target ids, capture ordinal, observed/source/session times, viewport,
warnings, request/resolved positions, scope). Pagination now terminates correctly
(next_offset present until the tail, absent on the last page — verified 361-frame
range paged 0→64→…→320→end), so the fix is purely payload weight, not looping.

Two directions: (1) a genuinely concise listing projection — frame_id +
resolved_position + session_time + byte_len is enough to drive follow-up fetches;
push the full provenance to `detail: expanded`/`full` or to fetch_source_frames.
(2) Lower the default page size, or advertise a recommended page size in the
schema, so a default listing fits a typical host result budget. Today the only
way through is jq/python over the saved oversized result file, which defeats the
"discover frame ids to drive region/filmstrip tools" workflow the listing exists
for.
