---
id: epic-state-aware-interaction-results-postcondition-core-fact-capture
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-postcondition-core
depends_on: [epic-state-aware-interaction-results-postcondition-core-domain-types]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# CDP fact capture and assembly

Checkpoint for Units 3-4 of the parent feature's design: widen the
`resolve_backend_node` probe (per-property-guarded JS returning
checked/expanded/selected/pressed/value-length) and carry `NodeStateFacts` on
`ResolvedNode`; in `execute_interaction_request_inner` add the bounded
pre-dispatch `location.href` read, unconditional passive lifecycle
subscription, post-action re-probe of the same backend node (failure →
`DetachedOrReplaced`; no target → `NotEvaluated`), post-URL from the
observation's `PageState`, and `from_facts` assembly into
`InteractionRecord::new`.

## Acceptance evidence

- Deterministic doubles: checkbox click `checked false→true`; link click
  without navigation `url_changed: false` + no lifecycle signal; fill
  `value_length_changed: true`; degraded post-probe leaves the action
  successful with unobserved facts; page-scoped `press_keys` reports
  `NotEvaluated` target with observed page facts.
- Probe/URL degradation never fails or delays a proven dispatch.
- Batch steps inherit postconditions with no batch-specific code.
- Workspace gate green.

## Implementation

Landed as designed:

- `crates/krometrail-cdp/src/control/snapshot.rs` — the probe JS is extracted
  to `NODE_STATE_PROBE` and widened with per-property guarded reads
  (`checked`/`ariaExpanded`/`selected`/`pressed`/`valueLength`; a tiny
  `try/catch` guard per fact plus an aria tri-state mapper where "mixed" and
  anything non-"true"/"false" degrade to null). `parse_node_state_facts`
  degrades per field and saturates oversized value lengths to `u32::MAX`
  (matching `LocatorSummary`'s selector-length convention). `ResolvedNode`
  carries `facts`; `probe_backend_node_facts` is the shared post-action
  re-probe where every failure degrades to `None`.
- `crates/krometrail-cdp/src/control/interaction.rs` — bounded pre-dispatch
  `location.href` read (`read_page_url`, silent + `throwOnSideEffect`, every
  failure degrades to `None`), unconditional passive lifecycle subscription
  drained non-blockingly at the observation point
  (`PageSignalReceiver::signal_observed`, new in `events/signals.rs`),
  post-action re-probe on the healthy observation path, post-URL from the
  observation's `PageState`, `from_facts` assembly into
  `InteractionRecord::new`. The `NAVIGATION_AWARE_WINDOW` wait remains gated
  on `wait_for_navigation` exactly as before (the gated subscription is
  untouched; the passive one is a second broadcast receiver).

Deviations / judgment calls:

- **HandleDialog skips the pre-URL read.** An open modal blocks the
  renderer's evaluation loop; a pre-dispatch `Runtime.evaluate` would sit in
  front of the very dialog handling that unblocks it. The URL fact degrades
  to unobserved for dialog handling.
- **Blocked/degraded observation paths skip the post probe and report the
  target as `NotEvaluated`** rather than probing a blocked renderer and then
  claiming `DetachedOrReplaced` from the inevitable timeout — that claim
  would be false for a node that merely sits behind a dialog. Only the
  healthy path evaluates the target; on that path a probe/transport failure
  maps to `DetachedOrReplaced` with unobserved after-facts per the design.
- **New `POSTCONDITION_PROBE_WINDOW` (2s)** bounds both silent reads so a
  stalled renderer degrades facts instead of delaying the interaction.
- **Scripted harness seam**: `ScriptedCdp` answers the silent
  `location.href` expression out of band (before the per-method queue) so
  the many existing interaction scripts stay stable; post-URL remains fully
  scriptable through the observation identity read, which is what the
  comparison consumes.

Tests: five new deterministic doubles in
`crates/krometrail-cdp/tests/verified_interactions.rs` (checkbox checked
delta, same-route click with no URL change and no lifecycle signal, fill
value-length change, degraded post-probe with preserved success, page-scoped
press_keys with observed page facts); inline probe-parsing degradation tests
in `snapshot.rs`; passive-drain semantics test in `events/signals.rs`.
Batch inheritance needed no code and is exercised by existing batch doubles
passing through the same path.

Gate: fmt, wire-enum schemas, check, full test suite (1188 passed, 0
failed), clippy -D warnings — all green.
