---
id: story-bound-agent-presentations
kind: story
stage: review
created: 2026-07-20
updated: 2026-07-20
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
---

Further bound default agent-facing presentations on dense pages. On the public Krometrail GitHub issues page, a concise snapshot serialized to roughly 12.5 KB for 48 action targets and a two-step batch response to roughly 17 KB because each target retained a complete reference and state list. On the MDN media-query guide, `list_page_assets {}` returned the full inventory at roughly 34 KB with no compact by-kind summary or page/limit control. Krometrail is usefully smaller than a full 31 KB MDN semantic snapshot, but routine results should keep the cheapest actionable rows and aggregate or paginate inventory-style surfaces while preserving explicit expansion paths.

## Acceptance

- Concise snapshots retain exact actionable references while omitting nonessential per-target state and staying under a substantially smaller byte budget.
- Concise page-asset responses summarize the complete acquired inventory by kind; expanded gives bounded useful rows and full remains the explicit complete projection.
- Batch and live-observation paths reuse the same canonical projections without a parallel acquisition path.

## Implementation notes

- Root cause: concise snapshots explicitly allowed 48 targets/12 KiB and repeated default-false accessible states; `list_page_assets` bypassed response projection and serialized every acquired row.
- Fix: concise targets are action-ranked at 24 rows/6 KiB with false boolean defaults omitted; expanded retains complete states at 48 rows/12 KiB. Asset projection derives counts by kind and separate source/presentation omissions from the canonical inventory, with 16-row/6 KiB concise and 64-row/16 KiB expanded bounds; full remains canonical.
- Shared behavior: actions, batches, and live observation already route their final snapshot through the same projector, so no batch-specific truncator or alternate acquisition path was added.
- Verification: `cargo test -p krometrail-mcp --lib --locked response::tests`; schema coverage proves frame document scope remains published.
