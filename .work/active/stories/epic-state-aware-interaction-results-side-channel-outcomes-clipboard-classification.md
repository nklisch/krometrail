---
id: epic-state-aware-interaction-results-side-channel-outcomes-clipboard-classification
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

## Implementation

Landed per the parent design:

- **`TransportError::Timeout`** (`transport/error.rs`): distinct
  sent-but-unanswered category; `is_retryable` stays false (reconnect
  behavior unchanged). `cdpkit.rs` maps `CdpError::Timeout => Timeout`
  instead of the previous collapse into `CommandFailed`, with a direct
  fault-injection test (`command_timeout_keeps_its_own_transport_category`)
  confirming the #8 root cause: the observed `command_failed` dispatch death
  was a command timeout — an unsettled `navigator.clipboard.readText()`
  promise — not a transport defect. The variant addition was compile-driven;
  the only exhaustive `TransportError` match in the workspace was the
  clipboard `transport_error_class` table (now `command_timeout`).
- **Classification** (`control/clipboard.rs` `clipboard_dispatch_error`):
  `Timeout` → `InteractionFailed` naming the unsettled operation and the
  pending-permission / OS-unfocused-window possibilities (class
  `command_timeout`), recovery: focus the window at the OS level, resolve
  any prompt, retry; `Protocol` (browser rejected the command) →
  `StaleReference` (destroyed document/isolated world), recovery:
  re-inspect and retry; `Disconnected` still propagates
  `BrowserDisconnected`; other classes keep the generic transport message.
  In-page typed errors (secure context / focus / unavailable /
  permission-denied via `exceptionDetails`) untouched. The pre-bridge world
  resolution steps keep their existing stale classification per the
  design's stated scope.
- **Record enrichment** (`session/evidence.rs`): the WriteClipboard record
  block is `InteractionPostcondition::clipboard_confirmed()` — a confirmed
  fact, since that projection only runs after the in-page bridge returned
  `true` — and the record still never contains clipboard text (asserted).
- Classification unit tests cover Timeout wording (names the unsettled
  operation and pending-permission possibility, no transport-error claim),
  Protocol → StaleReference, Disconnected → BrowserDisconnected, and the
  unchanged CommandFailed path. The gated real-Chrome clipboard
  qualification re-ran green as a regression check.

Gate: fmt, wire-enum guard, check, full workspace test, clippy -D warnings
green.
