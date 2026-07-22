---
id: epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts
kind: story
stage: done
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

## Implementation

Landed per the parent design with the following notes:

- **Core types** (`krometrail-core/src/browser/postcondition.rs`):
  `NewPageFact`, `NewPagePostcondition`, `DownloadFact`,
  `DownloadPostcondition`, `SideChannelSignals`,
  `PagePostcondition.main_frame_navigation_observed`,
  `InteractionPostcondition.{signals,new_pages,downloads,clipboard_write_confirmed}`,
  `MAX_SIDE_CHANNEL_FACTS = 4` with exact omission counts, over-cap wire
  rejection via `deserialize_validated`, `clipboard_confirmed()` /
  `attach_new_pages` / `attach_downloads`. `InteractionPostcondition` lost
  `Copy` as designed; all construction sites adapted compile-driven.
- **Signals**: broadcast payload became `PageSignal { kind, observed_at }`
  (pump-stamped monotonic time); `PageSignalKind::{WindowOpen,
  DownloadRequested, NavigationCommitted}`; `observed_count_between` /
  `signal_observed_between` fenced drains. `Page.frameNavigated` and
  `Page.navigatedWithinDocument` promoted to always-installed operation
  signals with main-frame filtering (`TargetEventRuntime.main_frame` recorded
  from committed main-frame navigations); `Page.windowOpen` and
  `Page.frameRequestedNavigation` (disposition `download`) installed as
  signal-only sources with silent install-failure degradation and no
  normalization/persistence/gap accounting.
- **Interaction path**: passive receivers for the three new signal kinds;
  one attribution fence (`signal_floor` at dispatch, `signal_ceiling` at
  observation-complete) shared by all passive drains; facts feed the extended
  `from_facts`.
- **Session seam**: `reconcile_targets_once` extracted from `wait_for_page`
  (reused there verbatim); `execute_operation` captures the pre-action page
  cursor for interactions, runs a 2s-bounded reconciliation pull after
  success and before `persist_result_evidence`, and attaches the new-page
  delta with opener matching. Batch steps inherit through the shared seam
  (asserted in the sequential-batch test).
- **Store**: schema v9 → v10; incompatible-version list, sqlite_schema
  assertions, and the runtime-smoke instance-database assertion updated; the
  store round-trip fixture now persists a fully-populated side-channel block.
- **MCP**: concise/expanded projection test updated for the extended block
  (projection itself was already field-driven).

Review-fix items folded in from the postcondition-core cross-model review
(attributed to that review, not this story's own findings):

1. **False detachment claim fixed**: `TargetNodeOutcome::Unobserved` added
   (probe attempted, could not observe); `DetachedOrReplaced` now requires a
   probe that ran and reported `connected: false`; a probe payload missing
   the boolean `connected` degrades to `None` (unobserved) instead of
   defaulting to a false detachment. The degradation double now locks in
   `Unobserved`, with new genuine-detachment and malformed-payload doubles.
2. **Signal fencing**: signals carry pump-stamped monotonic observation time;
   drains filter to the dispatch..observation interval. The legacy
   `navigation_lifecycle_observed` fact was kept as a separate fact (it
   reports lifecycle signals, not committed navigations) but is fenced
   identically through the same floor/ceiling — one fence authority, two
   facts. Consecutive-interaction leakage covered by the signals unit test
   (pre-floor and post-ceiling deliveries never count).
3. **Probe budget**: pre-dispatch URL read capped at 250ms
   (`PRE_URL_PROBE_WINDOW`); the post-action state probe now runs
   concurrently with the compositor rendezvous + live observation
   (`tokio::join!`), keeping its 2s cap as a concurrent bound with no serial
   latency.
4. **Future-schema test made version-agnostic**: seeds 9999 with stale cache
   content and asserts replacement with the current schema (no longer seeds
   the current version).

Test-support addition: `ScriptedCdp::fail_subscription` for
subscription-failure injection (silent signal-only degradation coverage).
`browser_events` per-session subscription count updated 12 → 14 for the two
signal-only sources.

Gate: fmt, wire-enum guard, check, full workspace test, clippy -D warnings —
all green.
