---
id: epic-agent-browser-operation-verified-interactions-keyboard-and-form-actions
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-verified-interactions
depends_on: [epic-agent-browser-operation-verified-interactions-dispatch-and-pointer-actions]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Keyboard and form actions

## Scope

Implement `Fill`, `PressKeys`, and `SelectOption` in `crates/krometrail-cdp/src/control/keyboard.rs` and `crates/krometrail-cdp/src/control/form.rs`, reusing the shared `execute_interaction` lifecycle and the extended resolver from the previous story.

## Deliverables

- Add `control/keyboard.rs`:
  - `Fill` (`FillMode::Replace`): focus the resolved element (click at center via the existing pointer helpers, or `Input.focus` if exposed; otherwise the click path), clear via Ctrl+A + Delete (or select-all + Backspace), then `Input.insertText({ text: value })`. `FillMode::Append` skips the clear and inserts after the current cursor. Element required (`ReferenceRequirement::Editable`); coordinate locators reject at construction in core.
  - `PressKeys`: for each `KeyChord` in the request, dispatch the parsed `KeySegment`s as `Input.dispatchKeyEvent` (rawKeyDown/char/keyUp). Modifier chords hold the modifier down across the named-key press and release it after. Element-targeted PressKeys focuses the element first; `None` locator uses the current focus.
  - One private static `KEY_DISPATCH` table maps `NamedKey` to CDP `{ key, code, location, windowsVirtualKeyCode }`. Single Unicode chars dispatch with `text: <char>`.
- Add `control/form.rs`:
  - `SelectOption`: resolve the element with `Selectable`, then a bounded `Runtime.callFunctionOn` (side-effecting, `throwOnSideEffect: false`, returns boolean success) that finds the matching `<option>` by `value`, index, or visible label, sets `selected`, and dispatches `input`/`change`. Unmatched value → `InvalidInput` (`select_value_not_matched`). Element required; non-`<select>` fails at the resolver.
- Wire the three actions into `PageControl::execute` via `execute_interaction` with action-specific `dispatch` closures. `Fill`/`PressKeys` honor `wait_for_navigation` (escalate `Settled` → bounded `NavigationAware`); `SelectOption` uses `Settled`.
- Sanitization redacts `Fill` value to length + 32-char preview and bounds `SelectOption` value length, per the parent feature's rules.
- Scripted tests: `Fill` Replace clears-then-inserts and Append skips clear; `PressKeys` produces the right `Input.dispatchKeyEvent` sequence for `Enter`, `Control+S`, `Shift+ArrowDown`, and a multi-char string; element-focus-before-press for element-targeted PressKeys; `SelectOption` value/index/label each produce the right `Runtime.callFunctionOn` and unmatched label → `InvalidInput`; non-editable/non-select fail at the resolver; sanitized parameters redact the right fields.

## Acceptance criteria

- [ ] `Fill` replaces (Replace) or appends (Append) the value of an editable control via `Input.insertText` after focusing and clearing; non-editable elements fail at the resolver with `ReferenceNotActionable` (action-specific message).
- [ ] `PressKeys` dispatches validated key chords as `Input.dispatchKeyEvent` sequences, supports modifier chords, and accepts either an element locator (focus first) or `None` (target-wide current focus).
- [ ] `SelectOption` sets the matched option on a `<select>` through a bounded, side-effecting `Runtime.callFunctionOn` and dispatches `input`/`change`; non-`<select>` targets fail at the resolver; unmatched values fail `InvalidInput`.
- [ ] Keyboard/form actions carry honest completion and reuse the shared post-action `LiveObservation`; sanitized parameters redact `Fill` value to length + bounded preview and bound `SelectOption` value length.
- [ ] `cargo fmt --all -- --check`, `cargo check -p krometrail-cdp --all-targets --locked`, `cargo test -p krometrail-cdp --lib --locked`, and `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` pass; the workspace gates remain green.

## Out of scope

- File upload and dialog actions (next story).
- Real-Chrome qualification and the standalone fixture (final story).

## Implementation notes

- Added keyboard and form action families behind the shared interaction executor; neither family re-resolves targets, allocates ids, or captures its own observation.
- Fill focuses the verified editable backend node, performs replace via Control+A/Delete when requested, and sends bounded text through `Input.insertText`; append preserves the current selection/cursor.
- PressKeys consumes the core-validated closed chord grammar, holds modifiers across the non-modifier key, emits raw-key/char/key-up sequences, and centralizes all named-key CDP metadata in one complete table.
- SelectOption resolves only a native selectable element, obtains its runtime object from the verified backend node, performs one bounded side-effecting option match/set, fires input/change, and returns `select_value_not_matched` without echoing option content.
- Verification passed: formatting, locked all-target CDP check, 80 CDP library tests (including registry-complete key mappings), and locked all-target CDP Clippy with warnings denied. Production-port scripted and real-browser effects are consolidated in qualification.
