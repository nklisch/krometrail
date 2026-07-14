---
id: epic-agent-browser-operation-verified-interactions-qualification
kind: story
stage: implementing
tags: [browser, agent-ux, testing]
parent: epic-agent-browser-operation-verified-interactions
depends_on: [epic-agent-browser-operation-verified-interactions-keyboard-and-form-actions, epic-agent-browser-operation-verified-interactions-upload-and-dialog]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Deterministic and real-browser qualification

## Scope

Add `crates/krometrail-cdp/tests/verified_interactions.rs` covering every action through the production connector port against the scripted transport (default suite) and against real Chrome (opt-in), plus a standalone dependency-free fixture page. Consolidate or extend existing test-support helpers rather than duplicating them.

## Deliverables

- Add `tests/fixtures/browser/verified-interactions/index.html`: normal/disabled/hidden buttons, text input + textarea + checkbox + `<select>` + `<input type=file>` + contenteditable, a draggable element with known HTML5 drag handlers, a scrollable container with known dimensions, a coordinate-clickable `<div>` with known position, and a button that opens `confirm()`/`prompt()` dialogs. Document it in `tests/fixtures/browser/README.md` as a target-only, dependency-free fixture (not a second Krometrail runtime).
- Extend `tests/support/scripted_cdp.rs` only where needed for interaction-specific responses (e.g. `Input.dispatchMouseEvent`/`Input.dispatchKeyEvent`/`Input.insertText`/`DOM.setFileInputFiles`/`Page.handleJavaScriptDialog` acknowledgements). Reuse the existing layout/ax-tree/frame-tree/png helpers from `page_observation.rs`.
- Add `tests/verified_interactions.rs` with deterministic tests asserting:
  - Exact `Input.dispatchMouseEvent` JSON for click/hover/drag/scroll (modifier bitmask, clickCount, document→visual viewport coordinate conversion).
  - Actionability routing: `Actionable`/`VisibleGeometry`/`Editable`/`Selectable`/`FileInput` each verified by a satisfying and a violating node-state response with the right stable code.
  - Coordinate hit-test: `Document.elementFromPoint` returns null → `InteractionFailed` (`no_hit_target`); non-null → dispatch proceeds.
  - Stale reference during interaction: snapshot generation replaced between snapshot and click → `StaleReference`.
  - Navigation-aware completion: `wait_for_navigation: true` consumes a `Page.lifecycleEvent` event within the bounded window; timeout without the event still resolves successfully with honest timing.
  - Key chord translation: `KeyChord` parsing produces the right `Input.dispatchKeyEvent` sequence for `Enter`, `Control+S`, `Shift+ArrowDown`, and a multi-char string.
  - Fill Replace/Append; SelectOption value/index/label and unmatched → `InvalidInput`.
  - File upload valid dispatch, missing → `NotFound`, non-file-input → `ReferenceNotActionable`.
  - Dialog `Accept`/`Dismiss` payload; "no dialog" → `NotFound`.
  - Sanitization redacts `Fill` value, dialog prompt, upload paths; never echoes CDP identifiers, backend node ids, object ids, or transport session ids.
  - Interaction record: id allocated from `IdSource`, dispatch/live-observation times ordered, locator summary kind matches, `parent_batch: None`, `outcome: Dispatched`.
  - Reconnect/stop completion: queued interaction commands during reconnect receive `BrowserDisconnected` without replay; queue closure receives `Cancelled`.
- Add opt-in real-Chrome tests (under `KROMETRAIL_REAL_CHROME_TESTS=1`) covering click/fill/press/select/hover/drag/scroll/upload/dialog, coordinate fallback (empty-space no-op + known-div success), and stale reference after dynamic replacement.
- Real-Chrome upload creates a temp file via the test (using the existing `tempfile`/`std::env::temp_dir` patterns from `chrome.rs`); the test owns the cleanup.

## Acceptance criteria

- [ ] Default deterministic tests protect the stable action/reference/error/sanitization seams without depending on Chrome timing.
- [ ] Production-connector Chrome tests cover click/fill/press/select/hover/drag/scroll/upload/dialog and the coordinate-fallback + stale-reference boundaries on Linux; platform/scale observations remain explicit.
- [ ] The fixture is target-only, dependency-free, documented, and introduces no second Krometrail runtime.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass.

## Out of scope

- macOS/high-DPI specific qualification (carry forward the page-observation policy: report measured scale honestly; do not invent unavailable display modes).
- MCP/CLI exposure of the interaction operations.

## Implementation blocker (2026-07-14)

Real-Chrome dialog qualification exposed a hard contract/transport blocker; the story remains `implementing` and is not marked done.

- The default deterministic interaction suite is green (8 `verified_interactions` tests), and the opt-in production-port workflow exercised click, fill replace/append, key chords, selection, hover, drag dispatch, offset/element scrolling, upload, coordinate success/no-hit, and stale references before the dialog boundary.
- The fixture can reproducibly open a modal from a production-port coordinate click. Evidence that the modal is active is that subsequent same-session `Runtime.evaluate` commands block until the transport command timeout.
- Despite the active modal, `Page.handleJavaScriptDialog` sent through the exact current flat target session returns Chrome protocol error `-32602: No dialog is showing`. Sending the command at browser scope returns `-32601` (method unavailable), confirming that browser scope is not a valid workaround.
- `Page.javascriptDialogOpening`/`Page.javascriptDialogClosed` named subscriptions did not provide a usable synchronization signal through the selected cdpkit raw event path in this scenario. Pre-dispatch subscription, bounded command/event races, renderer task checkpoints, and cancellation-aware retries were attempted without making the command observe the active dialog.
- No sleeps, assertion weakening, direct transport bypass in qualification, fake success, or MCP/batch expansion was accepted. Temporary source-error and pointer diagnostics were removed.

The blocked design assumption is that an active JavaScript modal opened while a flat-session input command is in flight can always be handled by a subsequent raw `Page.handleJavaScriptDialog` call on that same cdpkit session. The next pass must determine whether this is a cdpkit pending-command/session-routing limitation, requires persistent dialog state/event ownership in the supervisor, or needs a different qualified transport command path. Until that is proven against real Chrome, dialog handling cannot honestly satisfy the feature contract and the feature cannot advance to review.

Full workspace gates were not claimed: concurrent durable-memory work currently leaves `krometrail-store/src/index/deletion.rs` absent during `cargo fmt --all`, and a pre-existing capture registry test still expects 7 stream states after `PausedBudget` made the registry length 8. Neither unrelated area was edited.
