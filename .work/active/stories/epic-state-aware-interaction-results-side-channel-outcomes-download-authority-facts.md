---
id: epic-state-aware-interaction-results-side-channel-outcomes-download-authority-facts
kind: story
stage: implementing
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
