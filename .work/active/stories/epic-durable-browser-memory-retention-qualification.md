---
id: epic-durable-browser-memory-retention-qualification
kind: story
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: [epic-durable-browser-memory-retention-capture-wiring]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Small-Budget Retention Qualification

## Checkpoint

Qualify the integrated feature with real tiny-budget segment/SQLite/artifact fixtures and scripted CDP budget gating. Cover global age, overlapping pins, provenance invalidation, all-pinned pause/resume, one-open overhead, deletion crash replay, complete session deletion, cancellation, source-safe errors, and full workspace gates without touching unrelated active edits.

## Ordering

Final checkpoint. It depends on production capture wiring so evidence covers the composed behavior rather than isolated policy helpers.

## Acceptance evidence

- Deterministic tiny budgets evict exact oldest unpinned segments across sessions and stay within budget after reported bounded overhead.
- Pinned ranges remain readable; all-pinned storage pauses and resumes only after unpin/deletion frees enough bytes.
- Mixed-source artifacts are removed before provenance becomes incomplete; unaffected artifacts remain reproducible.
- Failure at every deletion phase plus reopen converges with no dangling row/file or under-reported usage.
- Deleting one populated session removes all of its data while preserving another session.
- Paused capture acknowledgement/gap/state/shutdown behavior is proven at the CDP boundary.
- Locked workspace format/check/test/clippy pass in an isolated clean worktree if unrelated primary-tree edits remain.
