---
id: epic-agent-browser-reliability-managed-session-lifecycle-pointer-activation
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-managed-session-lifecycle
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Foreground hidden pointer targets

## Checkpoint

Use Krometrail-owned CDP activation before pointer-like input on a hidden managed page, and return a specific actionable target-visibility error when bounded recovery cannot make the renderer visible.

## Exact implementation

Extend `BoundTarget` with the opaque browser target key and supervisor visibility. Add `prepare_pointer_target` in `crates/krometrail-cdp/src/control/interaction.rs`: for pointer, drag/drop, and scroll categories that are not known visible, send browser-scoped `Target.activateTarget`, session-scoped `Page.bringToFront`, and a bounded `document.visibilityState` recheck before locator resolution. Add the stable `ErrorCode::TargetHidden` core value with target context and recovery. Do not mutate selected-target identity or weaken reference/actionability checks.

## Acceptance evidence

- [ ] Visible targets retain their existing command sequence with no activation overhead.
- [ ] Hidden targets activate and verify visibility before pointer input; persistent hidden/unknown state dispatches no mouse event and returns `target_hidden`.
- [ ] Selection identity, stale-reference behavior, and CSS-hidden-element actionability remain unchanged.
- [ ] Deterministic scope/order tests pass, and an ignored real-browser test demonstrates pointer recovery without AppleScript or manual application focus.

## Ordering and boundary

This checkpoint is graph-independent. The interaction-semantics sibling owns locator scrolling and key/fill behavior; this checkpoint owns page-target visibility before pointer-like dispatch.

## Implementation evidence

- `BoundTarget` now retains opaque browser target identity and supervisor visibility.
- Pointer, drag/drop, and scroll preparation uses browser-scoped activation, session-scoped foregrounding, and bounded literal visibility verification; failure returns `target_hidden` before interaction identity or input dispatch.
