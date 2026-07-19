---
id: story-opener-click-degraded-observation
kind: story
stage: review
tags: [bug, browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Degrade opener click when popup steals the post-action observation

Residual from `feature-window-lifecycle-integrity` (live MCP qualification on
the final batch build, 2026-07-19): a click on a `window.open`-triggering
button now correctly opens the popup, which commits navigation and is adopted
with `opener_target_id` (`wait_for_page` matches — the headline fix works),
but the opener's click response is still the hard error "click failed: browser
rejected or could not complete the page observation command" even though the
input demonstrably dispatched. The feature's observation-degradation unit
covers the post-dispatch observation-unavailable path in
`control/interaction.rs`, yet this path — the post-action observation's
transport command failing while the popup steals focus/compositing
(`transport_error` in `control/mod.rs`) — still propagates as an operation
error, dropping the interaction record.

Repro: managed foreground session on a page whose button calls
`window.open(url, name, 'width=,height=')`; `click` on that button; observe
the hard error while the popup appears and is adopted.

## Fix shape

Route the post-action observation's transport-command failure on a dispatched
interaction through the same degraded-with-record shape the feature added for
observation-unavailable, keeping genuine dispatch failures hard.

## Acceptance

- [x] Deterministic double: dispatched click + post-action observation
      transport `command_failed` → degraded response carrying the interaction
      record and diagnostics, not an operation error.
- [x] Pre-dispatch transport failures still surface as hard errors.

Origin: `.work/backlog/idea-popup-opener-click-hard-error-residual.md`.

## Implementation notes

- Execution capability: host implementation, because the interaction boundary
  and its transport-error classifier are one cohesive change.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-cdp/src/control/interaction.rs`,
  `crates/krometrail-cdp/src/control/mod.rs`, and the deterministic
  scripted transport coverage in
  `crates/krometrail-cdp/tests/verified_interactions.rs`.
- Tests added: a dispatched click whose post-action page-observation command
  fails with `TransportError::CommandFailed`; the result retains its
  `InteractionRecord` and unavailable observation. Existing selector
  replacement coverage continues to assert that pre-dispatch failures emit no
  pointer input and remain hard errors.
- Simplification: post-action observation errors retain the existing typed
  observation/disconnected error when building the degraded record; only
  unexpected error codes are normalized to the bounded page-observation
  diagnostic. Dispatch code and preflight errors are unchanged.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all` and
  `CARGO_TARGET_DIR=/tmp/krometrail-target cargo test -p krometrail-cdp
  --test verified_interactions --features cdpkit-transport --locked` passed
  (14 tests; opt-in real-Chrome tests were not enabled).
