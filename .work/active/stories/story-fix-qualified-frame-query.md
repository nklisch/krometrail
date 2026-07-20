---
id: story-fix-qualified-frame-query
kind: story
stage: done
created: 2026-07-20
updated: 2026-07-20
tags: [browser, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
---

On `https://the-internet.herokuapp.com/iframe`, wait for TinyMCE to create its same-origin `about:blank` editor frame. `list_frames` then advertises the child as `same_origin_same_process`, but `query_page` scoped with that exact frame reference returns `no_match` for both `Your content goes here.` and the accessible rich-text-area identity. The Codex in-app browser snapshot exposes the iframe body and its paragraph, so the content is present and semantically observable. Diagnose why a qualified frame scope is empty and make the advertised frame/query contract agree.

## Acceptance

- A same-origin `about:blank` child frame advertised by `list_frames` returns its visible non-actionable semantic content from frame-scoped `snapshot_page`.
- Frame document qualification remains generation-, loader-, attachment-, and process-bound.
- A deterministic regression test fails on the released behavior and passes after the correction.

## Implementation notes

- Root cause: frame qualification and AX selection were correct. `query_page` intentionally filters non-actionable nodes because only exact actionable references may authorize later mutation; the TinyMCE reproduction was readonly content.
- Fix: `snapshot_page` now accepts the same qualified `document` scope and returns the selected frame's acquired semantic tree, including non-actionable text, while `query_page` keeps its action-reference boundary.
- Regression: a deterministic child-frame snapshot exposes readonly editor text with no invented reference and proves the child frame ID is sent to CDP.
- Verification: `cargo test -p krometrail-cdp --lib --locked same_origin_frame_snapshot_exposes_non_actionable_semantic_content`.

## Bounded inline review — 2026-07-20

- Verdict: approved. The change reuses the existing frame revalidation and snapshot registry, exposes no raw CDP identity, and does not broaden actionable-reference authority.
- Acceptance: readonly child-frame content is inspectable; query results remain mutation-safe; schema and skill guidance publish the intended split.
