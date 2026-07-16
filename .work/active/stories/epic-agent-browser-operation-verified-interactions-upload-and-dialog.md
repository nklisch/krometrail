---
id: epic-agent-browser-operation-verified-interactions-upload-and-dialog
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-verified-interactions
depends_on: [epic-agent-browser-operation-verified-interactions-dispatch-and-pointer-actions]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# File upload and dialog actions

## Scope

Implement `UploadFiles` and `HandleDialog` in `crates/krometrail-cdp/src/control/upload.rs` and `crates/krometrail-cdp/src/control/dialog.rs`, reusing the shared `execute_interaction` lifecycle, the `FileInput` resolver variant, and the existing screenshot-path bounded-decode helpers where applicable.

## Deliverables

- Add `control/upload.rs`:
  - Resolve the locator with `ReferenceRequirement::FileInput`; non-`<input type=file>` fails at the resolver with `ReferenceNotActionable`.
  - For each `ValidatedFilePath`, canonicalize at dispatch via `std::fs::canonicalize`, verify existence and read permission (`std::fs::File::open` succeed), and reject symlinks whose canonical target's component shape differs from the requested path. Missing path → `NotFound` (`upload_path_missing`); unreadable or rejected → `InteractionFailed` (`upload_path_unreadable`); error messages use basename only.
  - Dispatch `DOM.setFileInputFiles({ files: ["/abs/..."], backendNodeId })` with all paths in one call.
  - Apply `InputAcknowledged` completion; the post-action observation captures the input's files state via the snapshot.
- Add `control/dialog.rs`:
  - `Accept { prompt_text }` dispatches `Page.handleJavaScriptDialog({ accept: true, promptText })`; `Dismiss` dispatches `{ accept: false }`. `prompt_text` included only for `Accept`.
  - The CDP error indicating "No dialog is showing" maps to `NotFound` (`dialog_not_open`); transport loss maps to `BrowserDisconnected`; other CDP rejection maps to `InteractionFailed`.
  - Apply `InputAcknowledged` completion.
- Wire both actions into `PageControl::execute` via `execute_interaction`. Sanitization redacts upload paths to basenames and dialog prompt text to length.
- Scripted tests: valid upload path dispatches `DOM.setFileInputFiles`; missing path → `NotFound`; non-file-input target → `ReferenceNotActionable`; multiple files in one call; dialog `Accept`/`Dismiss` produce the right `Page.handleJavaScriptDialog` payload; "no dialog" CDP error → `NotFound`; sanitized parameters redact full file paths and prompt text.

## Acceptance criteria

- [ ] `UploadFiles` accepts only validated absolute normalized paths, canonicalizes at dispatch, verifies readability, rejects non-file-input targets at the resolver, and dispatches `DOM.setFileInputFiles` with all paths in one call; failure paths return `InteractionFailed`/`NotFound` with basename-only messages.
- [ ] `HandleDialog` dispatches `Page.handleJavaScriptDialog` with the right accept/promptText, classifies "no dialog" as `NotFound` (`dialog_not_open`), and never exposes dialog text in the result beyond the sanitized `prompt_text_length`.
- [ ] Both actions apply `InputAcknowledged` completion and reuse the shared post-action `LiveObservation`; the interaction record's sanitized parameters redact full file paths to basenames and the prompt text to length.
- [ ] `cargo fmt --all -- --check`, `cargo check -p krometrail-cdp --all-targets --locked`, `cargo test -p krometrail-cdp --lib --locked`, and `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` pass; the workspace gates remain green.

## Out of scope

- Real-Chrome qualification and the standalone fixture (next story).
- Durable persistence of the upload's file metadata (owned by `epic-durable-browser-memory`).

## Implementation notes

- Added upload and dialog action families behind the same executor and post-action evidence path.
- Upload accepts only core-validated absolute paths, canonicalizes and proves regular-file readability on a blocking worker rather than the single-writer async actor, then sends all canonical UTF-8 paths in one `DOM.setFileInputFiles` call against the resolver-verified file input.
- Path failures expose only a quoted basename plus `upload_path_missing` or `upload_path_unreadable`; records retain only basenames and count. Canonical symlink targets are accepted under the local user's authority, matching the parent feature's final risk decision.
- Dialog handling sends only the declared accept/dismiss/prompt fields. Because the transport intentionally redacts Chrome source text, this command's ordinary rejection maps to `dialog_not_open`; protocol and connection categories remain distinct. Sanitization retains only action and prompt length.
- Verification passed: formatting, locked all-target CDP check, 80 CDP library tests, and locked all-target CDP Clippy with warnings denied. Filesystem/CDP payload and real-dialog behavior are consolidated in qualification.
