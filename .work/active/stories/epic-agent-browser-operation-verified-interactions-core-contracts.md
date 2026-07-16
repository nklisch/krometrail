---
id: epic-agent-browser-operation-verified-interactions-core-contracts
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-verified-interactions
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Core interaction contracts and registry extension

## Scope

Extend the operation declaration macro and `BROWSER_OPERATION_REGISTRY` slice with optional action metadata, and add the infrastructure-free interaction value contracts in `crates/krometrail-core/src/browser/interaction.rs`. This story is the public contract foundation: it does not dispatch CDP input and does not touch the adapter.

## Deliverables

- Extend `define_browser_operations!` so each variant carries an optional `action: $action:expr` field of type `Option<&'static ActionDefinition>`. Update the existing five observation entries to pass `action: None`. Extend `BrowserOperationDefinition` with the `action` field.
- Add the nine interaction variants to the macro invocation: `Click`, `Fill`, `PressKeys`, `SelectOption`, `Hover`, `Drag`, `Scroll`, `UploadFiles`, `HandleDialog`. Each references a `const ACTION_<NAME>: ActionDefinition` declared beside the macro. Result type for all nine is `InteractionResult` (boxed).
- Add `interaction.rs` with the public types listed in the parent feature's Unit 1: `ActionCategory`, `ActionabilityRequirement`, `AcceptedLocator`, `CompletionKind`, `ActionDefinition`; `InteractionLocator`; `MouseButton`, `Modifiers`, `Modifier`, `NamedKey`, `KeySegment`, `KeyChord`; `FillMode`, `SelectValue`, `ScrollDelta`, `DialogAction`, `ValidatedFilePath`; the nine request structs; `InteractionOutcome`, `SanitizedParameters`, `LocatorSummary`, `LocatorKind`, `InteractionRecord`, `InteractionResult`; the `BrowserActionRequest` trait.
- Constructors enforce every documented invariant: `ClickRequest::click_count` 1..=3; element-required locators for `Fill`/`SelectOption`/`UploadFiles`; `PressKeysRequest::keys` 1..=32; `UploadFilesRequest::files` 1..=8; `ValidatedFilePath` absolute, no `..` after normalization, ≤32 components, ≤4096 bytes UTF-8; `ScrollDelta::ByOffset` finite; `SanitizedParameters` JSON object ≤4096 serialized bytes.
- `KeyChord::new` parses the input into `KeySegment`s, accepting `Modifier` tokens (`Alt`/`Control`/`Shift`/`Meta`, case-insensitive, also `Ctrl`/`Cmd`), `NamedKey` tokens (the closed `NamedKey` set), and single Unicode `char`s. Unknown multi-char tokens reject with `InvalidInput`. `segments()` returns the parsed form.
- Each interaction request implements `BrowserActionRequest`: `Click`/`Hover`/`Drag`/`Scroll`/`PressKeys` expose their locator (or `None` for target-wide `PressKeys`); `Fill`/`SelectOption`/`UploadFiles` expose their element locator; `HandleDialog` returns `None`. `sanitize()` redacts per the parent feature's rules: `Fill` value to length + 32-char preview, `HandleDialog` prompt to length, `UploadFiles` to basenames only, `SelectOption` value to length, others to safe summaries.
- Add `ErrorCode::InteractionFailed` with `default_retry: Safe` and a `default_recovery` text. Wire it into the `define_stable_enum!` declaration; verify it round-trips in the existing `every_error_code_round_trips_with_its_stable_name` test. Do **not** add it to `BROWSER_SESSION_CODES`/`is_browser_session_failure`.
- Re-export the new types from `crates/krometrail-core/src/browser/mod.rs` and `lib.rs`.
- `InteractionRecord` carries `parent_batch: Option<InteractionId>` (always `None` when constructed by this feature); deserialization round-trips through a wire struct + validated constructor that re-checks timing order (`started_at ≤ dispatch_time ≤ live_observation_time ≤ completed_at`), action kind/locator consistency, and sanitized payload shape.

## Acceptance criteria

- [ ] One declaration generates all fourteen operation variants, result associations, stable names, mutability/evidence metadata, action metadata (`None` for observation, `Some(&ACTION_*)` for interaction), and exhaustive registry tests.
- [ ] Core remains runtime/transport/filesystem independent; only `serde_json` is reused.
- [ ] Locators, modifiers, key chords (parsing + segment round-trip), fill modes, select values, scroll deltas, dialog actions, validated file paths, click/key/file counts, sanitized parameter size, interaction-record ordering, and Serde round-trip validate at constructors and Serde boundaries.
- [ ] Each interaction request implements `BrowserActionRequest`; sanitization redacts sensitive payloads to length/bounded preview and never echoes CDP identifiers.
- [ ] Existing observation requests, registry tests, and Serde payloads continue to round-trip unchanged.
- [ ] `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test -p krometrail-core --all-targets --locked`, and `cargo clippy -p krometrail-core --all-targets --locked -- -D warnings` pass.

## Out of scope

- Adapter dispatch, CDP input translation, the `IdSource` plumbing into `PageControl`, real-browser qualification. Those land in later stories.
- Durable persistence of `InteractionRecord` (owned by `epic-durable-browser-memory`).
- MCP schemas or batch composition (owned by sibling features).

## Implementation notes

- Extended the existing operation declaration from 13 to 22 variants. Lifecycle and observation entries remain in the same registry; all nine interactions carry generated action metadata rather than introducing a parallel action list.
- Adapted the design to the approved lifecycle contract by using `PageSelection` on interaction requests. Direct target and selected-page requests therefore share the same session-owned resolver as every existing page operation.
- Added validated, Serde-safe core values and request constructors, closed key-chord parsing, bounded file-path and collection inputs, sanitized parameter envelopes, interaction records, and `interaction_failed` recovery semantics without filesystem or transport dependencies.
- Sensitive sanitization keeps the bounded fill preview specified by design, prompt lengths, and upload basenames only. Full paths and prompt text do not enter records.
- Verification passed: formatting, locked all-target core check, 59 core tests, and locked all-target core Clippy with warnings denied.
