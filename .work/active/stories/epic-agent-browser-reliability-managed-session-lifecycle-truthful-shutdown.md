---
id: epic-agent-browser-reliability-managed-session-lifecycle-truthful-shutdown
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

# Report managed shutdown from remaining authority

## Checkpoint

Replace historical failure aggregation with a private shutdown report that distinguishes ancillary degradation from a resource Krometrail still owns. A failed capture stream, drain/flush failure, detach failure, close-command failure, or deadline exhausted after verified release cannot by itself produce `shutdown_incomplete`.

## Exact implementation

Implement `ShutdownQuality`, `RemainingResource`, `ShutdownReport`, and the `perform_shutdown(...) -> Result<ShutdownReport>` boundary in `crates/krometrail-cdp/src/session/shutdown.rs`. Make `ManagedChromeProcess::force_kill_now` report verified completion without weakening process-group ownership checks. Update runtime/reconnect mappings and add the `BrowserStopOutcome::ManagedBrowserClosedDegraded` core variant. Remove the capture pipeline's historical `CaptureStreamState::Failed` stop-completion check while preserving accepted-frame abandonment and gap evidence.

## Acceptance evidence

- [ ] A historically failed stream plus complete resource release returns a success or degraded-success stop outcome, never `shutdown_incomplete`.
- [ ] Ancillary phase failure plus verified process/profile/transport release returns `managed_browser_closed_degraded` and permits same-profile restart.
- [ ] A true incomplete result names the safe remaining resource class and retains its authority long enough for bounded cleanup rather than silently dropping it.
- [ ] Existing `managed_browser_closed` and `detached` serialized values remain unchanged, stop remains idempotently cached, and the aggregate deadline is still consumed once.

## Ordering and boundary

This is the first feature checkpoint because it owns the riskiest process/profile release invariant. Capture health remains owned by the capture-outcomes feature; this checkpoint only decides whether browser lifecycle cleanup is complete.

## Implementation evidence

- Shutdown now returns clean/degraded quality separately from remaining process/profile authority and retains a process/profile guard when force cleanup cannot verify completion.
- A terminal failed capture stream no longer poisons stop completion; deterministic aggregate-deadline tests now prove forced cleanup with released authority is degraded success.
