---
id: story-align-frame-access-labels
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Align list_frames access labels with actual frame-scope behavior

## Brief

`list_frames` labeled a same-process `srcdoc` iframe (inside a `data:` main document)
`access: "indeterminate"`, yet a frame-scoped `query_page` against that frame's reference
succeeded and returned actionable references. The documented contract says indeterminate
frame scope fails explicitly. Either the access classifier under-reports qualification for
same-process opaque-origin frames (label should be a qualified level), or the query gate
under-enforces (should have rejected). Determine which side is authoritative, align the
other, and cover with a frame fixture test. Behavior observed is better than documented —
prefer upgrading the label over breaking working queries, unless the gate's leniency is
unsound for genuinely cross-process frames.

## Acceptance

- Classifier and query gate agree for: same-origin frame, same-process opaque-origin
  (srcdoc/data) frame, cross-origin out-of-process frame, stale frame reference.
- `list_frames` labels match what a subsequent frame-scoped query actually does.
- Docs/skill text state the final contract.
