---
id: epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-side-channel-outcomes
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Popup and navigation facts

Design checkpoint 1 of the side-channel feature (Units 1, 2, 3, 4-pages, and
7-schema/projection in the parent design):

- Extended postcondition domain types in
  `crates/krometrail-core/src/browser/postcondition.rs`: `NewPageFact`,
  `NewPagePostcondition`, `DownloadFact`, `DownloadPostcondition`,
  `SideChannelSignals`, `main_frame_navigation_observed` on
  `PagePostcondition`, `clipboard_write_confirmed`, `MAX_SIDE_CHANNEL_FACTS`
  cap + exact omission counts, validated wire decoding.
- Signal plumbing in `events/signals.rs` / `events/domain.rs`: new
  `PageSignalKind::{WindowOpen, DownloadRequested, NavigationCommitted}`,
  `observed_count`, operation-signal promotion of `Page.frameNavigated` /
  `Page.navigatedWithinDocument`, signal-only installs for `Page.windowOpen`
  and `Page.frameRequestedNavigation` (disposition `download`), main-frame
  filtering in the pump.
- Passive attempt-signal collection in `control/interaction.rs` feeding
  `InteractionPostcondition::from_facts`.
- Session-layer post-dispatch reconciliation in `session/operations.rs`:
  extracted `reconcile_targets_once` shared with `wait_for_page`, pre-action
  page-cursor capture, bounded (2s) post-dispatch pull, new-page delta with
  opener matching, record enrichment before `persist_result_evidence`.
- Store `CURRENT_SCHEMA_VERSION` 9 → 10; store round-trip and MCP
  response-shape test updates.

## Acceptance evidence

- Core cap/omission and wire-rejection tests; serde round-trips.
- Pump tests for main-frame and disposition filtering; `observed_count`
  drain; silent signal-only install degradation.
- Doubles: popup delta with `opener_matched: true`; empty-delta honesty;
  reconciliation fault injection → `new_pages: None` with the action still
  succeeding; batch-step inheritance; same-URL committed navigation
  (`url_changed: Some(false)` + `main_frame_navigation_observed: Some(true)`).
- Store decodes a populated block at v10; concise response carries the block.
- Full workspace gate green (fmt, wire-enum schema guard, check, test,
  clippy -D warnings).

## Ordering constraints

First checkpoint — the download and clipboard checkpoints attach to the
types and the enrichment seam this story lands.
