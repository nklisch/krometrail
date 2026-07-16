---
id: epic-durable-browser-memory-retention-qualification
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: [epic-durable-browser-memory-retention-capture-wiring]
release_binding: 1.0.0
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

## Implementation evidence

- Added small-budget qualification for global cross-session age ordering, overlapping pins, all-pinned pause/resume, open-segment overhead, unpolled append behavior, and scoped session deletion with a surviving session.
- Extended provenance qualification with a mixed-source artifact that is removed before either source frame can become falsely reproducible, while an artifact sourced only from the surviving segment remains present.
- Extended deletion-journal reopen qualification to assert exact pending bytes before replay and zero pending/segment usage after both prepared and metadata-removed replay phases.
- The integrated CDP pause/resume and bounded shutdown coverage remains in `crates/krometrail-cdp/src/capture/tests.rs` from the capture-wiring checkpoint; focused capture tests pass alongside the new store qualification.
- Focused verification: `cargo test -p krometrail-store --test retention_small_budget --locked -- --nocapture`, `cargo test -p krometrail-store --lib --locked -- --nocapture`, and `cargo test -p krometrail-cdp --lib capture::tests --locked -- --nocapture` all pass.
- The primary tree's workspace test remains contaminated by unrelated verified-interactions WIP; its isolated clean-worktree gate is recorded on the parent feature after this checkpoint commit.
