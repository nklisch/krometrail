---
id: epic-state-aware-interaction-results-side-channel-outcomes-clipboard-classification
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

# Clipboard classification and record enrichment

Design checkpoint 3 of the side-channel feature (Unit 6 plus the
WriteClipboard record wiring from Unit 4 step 4 in the parent design):

- `TransportError::Timeout` variant (`transport/error.rs`) with
  `cdpkit.rs` mapping `CdpError::Timeout => Timeout`; `is_retryable` keeps
  Timeout non-retryable; exhaustive matches updated compile-driven.
- Clipboard dispatch classification (`control/clipboard.rs`): Timeout →
  `InteractionFailed` naming the unsettled operation and possible pending
  clipboard permission decision / OS-unfocused window (class
  `command_timeout`); Protocol (browser command rejection) →
  `StaleReference` (destroyed document/isolated world); Disconnected
  unchanged; in-page typed errors untouched.
- `evidence.rs` WriteClipboard projection uses
  `InteractionPostcondition::clipboard_confirmed()` instead of
  `unobserved()`.
- #8 root-cause conclusion carried in the parent feature's Design
  decisions: the observed `command_failed` was a command timeout
  (unsettled `readText` promise), not a transport defect.

## Acceptance evidence

- Classification tests: Timeout message names the unsettled operation and
  pending-permission possibility and no longer claims a transport error;
  Protocol → `StaleReference`; Disconnected → `BrowserDisconnected`.
- WriteClipboard record carries `clipboard_write_confirmed: Some(true)` and
  still never contains clipboard text.
- Full workspace gate green.

## Ordering constraints

Depends on the popup/navigation checkpoint only for the
`clipboard_write_confirmed` record field; independent of the download
checkpoint.
