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
