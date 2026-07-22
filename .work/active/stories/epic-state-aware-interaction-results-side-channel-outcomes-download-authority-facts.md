---
id: epic-state-aware-interaction-results-side-channel-outcomes-download-authority-facts
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-side-channel-outcomes
depends_on: [epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Download authority and facts

Design checkpoint 2 of the side-channel feature (Units 5, 4-downloads, and
7-qualification in the parent design):

- `LazyManagedDownloadAuthority` → eagerly-activated
  `ManagedDownloadControl` (`session/downloads.rs`): `activate(transport)`
  called at managed session start in `session/mod.rs` (best-effort; failure
  stores the unavailable error without failing start), sync `cursor()` and
  `begun_after()` accessors, `Entry.begun_sequence` retained at begin.
- Never-absent cursor contract mirroring pages: `next_sequence` seeded at 2,
  `DownloadInventory.cursor: DownloadSequence` (non-optional),
  `WaitForDownloadRequest.after: DownloadSequence` (required). Delete the
  `activated()` "call list_downloads first" trap and the Option-cursor
  special cases. Regenerate canonical wire schema artifacts; update the
  `wait_for_download` tool guidance; roll `docs/SPEC.md` Local Data forward
  in the same stride.
- Download delta attachment in the `execute_operation` enrichment seam:
  `begun_sequence > cursor_before` → `DownloadPostcondition`; attach-mode or
  unavailable authority → `downloads: None`.
- One gated real-Chrome qualification (`KROMETRAIL_REAL_CHROME_TESTS`):
  `<a download>` click → postcondition download fact +
  `wait_for_download(after: cursor_before)` completes; `window.open` click →
  `new_pages` fact with opener match + `window_open_attempts >= 1`.

## Acceptance evidence

- Activation ordering preserved (subscribe before enable); activation
  failure degrades: session starts, explicit ops report the stored error,
  interaction facts report `None`.
- Empty inventory reports `cursor == 1`.
- Doubles: injected `downloadWillBegin` after the pre-action cursor appears
  in the postcondition delta; a pre-action download progressing during the
  interaction does not (begun-sequence discipline); attach-mode `None`.
- Lazy-behavior tests retired/rewritten for the eager contract.
- Wire schema guard passes with the required `after`; gated qualification
  passes against local Chrome.
- Full workspace gate green.

## Ordering constraints

Depends on the popup/navigation checkpoint for the postcondition types, the
`DownloadRequested` signal, and the enrichment seam.

## Implementation

Landed per the parent design:

- **Eager activation** (`session/downloads.rs`, `session/mod.rs`):
  `LazyManagedDownloadAuthority` collapsed to `ManagedDownloadControl`
  (`OnceLock` active slot). `activate(transport)` runs at managed session
  start before `SessionShared` is published, preserving
  subscribe-before-enable ordering; failure stores the unavailable error and
  never fails start (logged, degraded). The `activated()` "call
  list_downloads before triggering a download" trap and the lazy special
  case are deleted; `list` is now synchronous and never activates.
- **Never-absent cursor**: `next_sequence` seeded at 2 so sequence 1 is the
  empty-inventory cursor (mirrors the page cursor);
  `DownloadInventory.cursor: DownloadSequence` (non-optional);
  `WaitForDownloadRequest.after: DownloadSequence` (required — breaking by
  design under Current Contract Discipline, no compat alias); the
  `after.map_or(0, …)` special case deleted. `wait_for_download` registry
  description now names both cursor sources; SPEC.md Local-surface prose
  rolled forward in the same stride.
- **Delta discipline**: `Entry.begun_sequence` retained at begin;
  `begun_after(cursor)` filters on begin ordering so a pre-action download's
  later transitions (which bump `public.sequence`) are never attributed to
  the current interaction. Facts report the download's current state.
- **Enrichment seam**: `execute_operation` captures the download cursor
  pre-dispatch alongside the page cursor and attaches
  `DownloadPostcondition::from_observed` after success, before evidence
  persistence — a lock-read only, no browser round-trip. Attach-mode or
  activation-failed sessions carry `downloads: None`.
- **Tests**: lazy-behavior tests rewritten for the eager contract
  (activation ordering, once-only activation, failure degradation with the
  stored error and absent cursor); `begun_after` begin-ordering discipline
  double; empty-inventory cursor == 1; attach-mode `downloads: None`
  asserted in the scripted interaction test. The full interaction wiring is
  covered by the new gated real-Chrome qualification
  (`opt_in_real_chrome_qualifies_download_and_popup_side_channel_facts` with
  a new `side-channel` browser fixture served by the qualification fixture
  server): download-link click → download delta anchored on the pre-action
  cursor + `wait_for_download(after: cursor_before, terminal)` completes
  with a completed download; `window.open` click → `window_open_attempts >=
  1` and opener-matched popup (via the delta, or via
  `wait_for_page(after: cursor_before)` under adoption latency, per the
  design's attempt/no-outcome honesty).
- No launcher fake exists for scripted managed sessions, so the
  injected-`downloadWillBegin`-through-`execute_operation` double is covered
  at the authority seam (`cursor`/`begun_after`, exactly what the seam
  calls) plus the tier-3 real-Chrome path — consistent with
  layered-cdp-qualification.

Gate: fmt, wire-enum guard, check, full workspace test, clippy -D warnings
green; gated real-Chrome qualification passed against local Chrome
(google-chrome-stable).
