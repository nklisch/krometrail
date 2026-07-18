---
id: compact-live-observations-deduplicate-warnings
kind: story
stage: done
tags: [agent-ux, diagnostics]
parent: compact-live-observations
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Deduplicate equivalent observation warnings

## Checkpoint

Structurally identical top-level warnings are logged and returned once in first-seen order, while meaningfully distinct errors remain separate.

## Acceptance evidence

- Dialog-blocked observation emits one warning for three unavailable nested components.
- Same-code/different-context warnings are not collapsed.

## Implementation notes

- `Projection::degrade_with_stage` now compares each warning by full structural equality before either emitting its diagnostic event or retaining it in the response.
- Exact clones retain first-seen order and produce one trace event. Same-code warnings with different messages or other structural fields remain distinct.
- The shared behavior also coalesces repeated terminal-capture warnings without removing the current-state image or changing degraded/failure composition.

## Verification

- Red regression: `cargo test -p krometrail-mcp response::tests::equivalent_warnings_are_logged_and_retained_once_without_collapsing_same_code --locked` failed before implementation with four retained warnings instead of the two distinct warnings.
- `cargo test -p krometrail-mcp response::tests::equivalent_warnings_are_logged_and_retained_once_without_collapsing_same_code --locked` — passed; two unique warnings produced two diagnostic events in first-seen order.
- `cargo test -p krometrail-mcp response::tests::degradation_wait_timeout_page_anchor_and_batch_failure_remain_distinct --locked` — passed; the cloned three-component live-observation warning is returned once while all nested components remain unavailable.
- `cargo test -p krometrail-mcp response::tests::failed_capture_degrades_success_without_removing_current_image --locked` — passed with duplicate capture statuses coalesced.
