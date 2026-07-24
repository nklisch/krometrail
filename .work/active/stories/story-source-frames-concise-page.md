---
id: story-source-frames-concise-page
kind: story
stage: done
tags: [mcp]
parent: null
depends_on: []
release_binding: 1.6.1
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Concise list_source_frames page projection

## Brief

`list_source_frames` at default (concise) detail floods agent context: a
default page of 64 rows emits 64 individual `krometrail://` resource links plus
the full per-frame row array. For the routine "walk the resolved order and pick
frame ids" use, an id-focused projection is enough; the resource links and full
row detail belong at higher detail levels or in `fetch_source_frames`.

Direction (canonical-result-projection pattern: project after canonical
acquisition, preserve outcomes and drill-down authority):

- Concise detail: keep the bounded chronological rows (frame id, session_time,
  byte length, media type, resolved position), the continuation offset, and the
  exact omission counts — but stop publishing one response resource link per
  listed frame. A single drill-down note (fetch_source_frames or the canonical
  URI shape) preserves access authority.
- Expanded/full detail: retain current behavior including per-frame resource
  links and provenance.
- No change to `fetch_source_frames`, which stays the exact byte-read path.

## Acceptance criteria

- [ ] Concise `list_source_frames` response for a 64-row page carries no
      per-frame published resources; expanded detail still does.
- [ ] Row data, `next_offset`, and omission counts are unchanged at every
      detail level; detail never changes acquisition or outcomes.
- [ ] Existing wire/schema checks stay green (`bash
      scripts/check-wire-enum-schemas.sh`); schema regenerated if the response
      shape declaration changes.
- [ ] A test pins the concise projection (no per-frame resources) and the
      expanded projection (resources present) for the same acquired page.

## Implementation notes

- Changed `crates/krometrail-mcp/src/response.rs` so concise
  `list_source_frames` projects rows without per-frame response resources;
  expanded and full retain the resource links.
- Added `response::tests::source_frame_listing_projects_rows_and_resources_by_detail`,
  which compares the same page's compact row fields, continuation offset, and
  omission count across concise and expanded projections.
- No response-shape declaration changed, so no schema regeneration was needed.
- Full gate passed: `cargo fmt --all -- --check`,
  `bash scripts/check-wire-enum-schemas.sh`, locked workspace check/test, and
  clippy with `-D warnings`. The test gate required the permission-enabled
  rerun because four CDP tests need local networking/process permissions in this
  environment.

## Review

Bounded fresh-context review: accepted, no blockers. Non-blocking notes: full
detail covered by code shape rather than a direct assertion; no explicit
drill-down note added at concise (drill-down authority preserved via per-row
frame ids and next_offset).
