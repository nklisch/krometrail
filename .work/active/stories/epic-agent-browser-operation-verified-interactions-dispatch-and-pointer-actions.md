---
id: epic-agent-browser-operation-verified-interactions-dispatch-and-pointer-actions
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-verified-interactions
depends_on: [epic-agent-browser-operation-verified-interactions-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Interaction dispatch foundation and pointer actions

## Scope

Build the shared interaction executor in `crates/krometrail-cdp/src/control/interaction.rs`, extend the shared resolver with action-specific actionability, plumb the session-wide `IdSource` into `PageControl`, and implement the pointer action family (click, hover, drag, scroll) in `control/pointer.rs`. This story delivers the central abstraction every other action family reuses.

## Deliverables

- Extend `SnapshotRegistry::resolve` (and `resolve_selector`) with the new `ReferenceRequirement` variants `Editable`, `Selectable`, `FileInput`. Replace the existing `Runtime.callFunctionOn` body with the richer fact set documented in the parent feature's trickiest-unit section (`{connected, visuallyHidden, interactionBlocked, tagName, inputType, isEditable, isSelect, isFileInput}`). `validate_node_state` consumes the action-specific subset; the common connected/not-hidden floor stays. The existing `VisibleGeometry`/`Actionable` behavior (including the disabled-but-visible path used by screenshots) is preserved.
- `ResolvedNode` exposes `backend_node_id` (in addition to `document_quad`) for downstream CDP calls.
- Add `crates/krometrail-cdp/src/ids.rs` with `UuidIdSource` implementing `krometrail_core::IdSource` (yields `IdValue::from_uuid(Uuid::new_v4())`). Export it from `lib.rs`.
- `PageControl::new` takes `ids: Arc<dyn IdSource>` in addition to the existing clock/session_id/session_origin; the field is stored. `ProductionBrowserConnector::new` defaults `ids` to `Arc::new(UuidIdSource)`; `with_capture` overrides with the capture assembly's source (mirroring the clock pattern). `connect` passes the source into `PageControl::new`.
- Add `control/interaction.rs` with the shared `execute_interaction` lifecycle documented in Unit 2 of the parent feature: allocate id → resolve locator (element via the resolver, coordinate via fresh layout + `Document.elementFromPoint`, none → `TargetWide`) → build partial `InteractionRecord` → call action-specific `dispatch` closure → apply `CompletionKind` (`InputAcknowledged`/`Settled`/`NavigationAware`) → run the same `inspect → snapshot → viewport-screenshot` sequence as `observe_live` → finalize record + return `InteractionResult`.
- `PageControl::execute` extends its `match` to route `Click`/`Hover`/`Drag`/`Scroll` (this story) and the remaining interaction variants (later stories, return a stable `Unsupported` until their stories land) through `execute_interaction` with action-specific `dispatch` closures.
- Add `control/pointer.rs` translating the four pointer actions to `Input.dispatchMouseEvent` exactly as specified in Unit 2: click (mouseMoved→mousePressed→mouseReleased with button/clickCount/modifiers bitmask), hover (mouseMoved only), drag (mouseMoved→mousePressed→bounded interpolated moves→mouseReleased), scroll-by-offset (`type: mouseWheel` with deltaX/deltaY at viewport center) and scroll-to-element (`DOM.scrollIntoViewIfNeeded({ backendNodeId })`).
- Coordinate conversion reuses the screenshot clip conversion path: fresh `Page.getLayoutMetrics` → `cssLayoutViewport.pageX/pageY`; visual viewport offset applied for CDP `Input.dispatchMouseEvent.x/y`. Non-finite converted coordinates return `InvalidInput`.
- `NavigationAware` completion subscribes to `Page.lifecycleEvent` for one bounded window when `wait_for_navigation: true` on `Click`; timeout without the event resolves the action successfully (post-action observation captured) and the timing is recorded.
- Reconnect/stop paths answer queued `Execute` interaction commands without dropping senders or replaying input (extend the existing supervisor patterns if needed; they should already cover interaction requests because the routing is identical).
- Scripted tests in `crates/krometrail-cdp/src/control/tests.rs` (or a sibling test module) for: exact `Input.dispatchMouseEvent` JSON per action; element actionability routing for `Actionable`/`VisibleGeometry`; coordinate hit-test null → `InteractionFailed` (`no_hit_target`); stale reference during click → `StaleReference`; navigation-aware completion consuming `Page.lifecycleEvent`; interaction-record allocation/timing/locator summary; reconnect/stop completion without replay.

## Acceptance criteria

- [ ] `PageControl` carries an `IdSource`; connector plumbing shares the capture source or falls back to `UuidIdSource`; the supervisor command path remains single-writer and reconnect/stop paths answer queued interaction commands without dropping senders or replaying input.
- [ ] Element-targeted pointer actions resolve through the shared resolver with `Actionable`; coordinate-targeted actions perform one `Document.elementFromPoint` hit-test and fail `InteractionFailed` (`no_hit_target`) on empty hit.
- [ ] CDP `Input.dispatchMouseEvent` parameters are exact and finite for click/hover/drag/scroll; drag interpolates a bounded number of intermediate moves; element scroll uses `DOM.scrollIntoViewIfNeeded`.
- [ ] `Click`/`Hover`/`Drag` apply `Settled` completion; `Scroll` applies `InputAcknowledged`; `Click` honors `wait_for_navigation` by escalating to bounded `NavigationAware`.
- [ ] Every interaction returns an `InteractionResult` with a fully-populated `InteractionRecord` (id, context, dispatch/live-observation times, sanitized params, locator summary, `outcome: Dispatched`) and an honest partial `LiveObservation`.
- [ ] `cargo fmt --all -- --check`, `cargo check -p krometrail-cdp --all-targets --locked`, `cargo test -p krometrail-cdp --lib --locked`, and `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` pass; the workspace gates remain green.

## Out of scope

- Keyboard (`Fill`, `PressKeys`) and form (`SelectOption`) actions (next story).
- Upload and dialog actions (story after).
- Real-Chrome qualification and the standalone fixture (final story).
