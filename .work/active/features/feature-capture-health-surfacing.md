---
id: feature-capture-health-surfacing
kind: feature
stage: review
tags: [agent-ux, browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Truthful capture health on the status and start surfaces

## Brief

When the capture writer failed terminally during the seventh shakedown, most
tools reported it correctly: `wait`, `fill`, `batch`, and `stop_browser` all
returned a `capture_failed` warning carrying the persistence detail and the
recovery text "restart the Krometrail MCP process, then start a new browser
session".

Two surfaces did not, and they are the two an agent trusts most:

- **`browser_status`** returned top-level `state: "ready"`,
  `budget_state: "available"`, `recording_blocked: false`, and
  `warnings: []`, with the terminal failure buried two levels down in
  `capture[0].failure`. The tool whose entire purpose is reporting session
  health is the one that does not warn.
- **`start_browser`** reported `state: "capturing"` with `warnings: []` on a
  session that was already doomed — the very next operation on that new session
  returned `capture_failed`. An agent starting a session gets a green light and
  captures into the void.

The evidence contract is that an agent can trust retained temporal evidence to
exist. A health surface that reports `ready` while the writer is terminally dead
breaks that contract more severely than the underlying failure does, because it
removes the agent's ability to notice.

## Simplification opportunity

The `capture_failed` warning projection already exists and is correct on the
control surfaces. This should reuse that one projection rather than adding a
second status-specific failure shape — one warning source, applied consistently.
Prefer making the shared response projection emit it for every tool over
per-tool opt-in, so a future tool cannot silently omit it.

Fold in if cohesive:
- `idea-harden-session-edge-semantics` — structured recovery field for
  `SubscriberLag` rather than message-only guidance, and idempotent
  `BrowserSessionPort::stop()` returning the previously observed terminal
  outcome instead of `cancelled`.

## Acceptance

- `browser_status` surfaces terminal capture failure in `warnings` and in a
  top-level state that does not read as healthy.
- `start_browser` does not report a healthy capturing session when the writer is
  in a terminal state; it reports the failure and the recovery path.
- `oldest_retained` / `newest_retained` no longer compare session-relative times
  across different sessions (observed: oldest > newest).
- A regression test asserts no tool reports healthy capture while the writer is
  terminal.

## Architectural choice

Capture health is applied **on the shared response exit**, not per tool.

Every MCP tool — control, lifecycle, temporal, progressive, evidence — funnels
through `into_call_tool_result`. That function now *requires* a capture-health
argument, so the compiler refuses to let a route reach the wire without stating
what it knows about the writer. A tool added next year inherits the warning
without its author having to know the warning exists. That was the whole point
of the item: `browser_status` and `start_browser` were not broken by a bug in
those two handlers, they were broken because warning emission was opt-in and two
handlers never opted in.

Rejected alternatives:

- **Mutating the serialized envelope in `KrometrailMcpServer::call_tool`.** One
  even-more-central place, and there is precedent (`attach_diagnostics`). But it
  would have re-derived status, warnings, and summary from loose JSON instead of
  the typed `ToolResponse`, and it cannot read the pre-transition health that
  `stop_browser` needs. Rejected as less truthful for a marginal gain in reach.
- **Adding a status-specific "capture unhealthy" field.** Rejected outright:
  the item asks for one warning source, and a second failure shape would give an
  agent two places to look and two ways to disagree.

## Design decisions

- **One warning source.** `add_capture_warnings` is gone. `capture_failed`
  now enters a response in exactly one place, `apply_capture_health`, which
  dedupes against warnings already present, degrades `succeeded` to `degraded`,
  and rewrites the summary to name the loss ("succeeded, but retained temporal
  evidence is unavailable"). `map_operation_result_with_capture_and_novelty` lost
  its `capture_statuses` parameter and its name shortened accordingly.
- **Target scoping is preserved.** The warning still filters to the response's
  target when one is identifiable, now resolved from the interaction anchor, the
  result pointer, *or* the error context — the last of which recovers the scope
  that `visible_error_with_capture` used to apply by hand.
- **`stop_browser` reads health before the transition.** By the time a stop
  outcome is mapped, the session that owned the failing writer is gone. Every
  other lifecycle tool reads health *after* its transition, which is precisely
  what makes `start_browser` unable to hand back a green light on a session whose
  writer is already terminal.
- **Retained bounds state their own comparability.** `session_time` is
  session-relative, so two endpoints drawn from different sessions are simply not
  orderable — this is why the live run showed oldest `126065361437` > newest
  `118028908063`. `oldest_retained`/`newest_retained` are replaced by one
  `retained_bounds` object carrying both endpoints, a `comparable_scope` flag,
  and a `span_nanos` that is present *only* within a comparable scope. Concise
  and expanded status gained the field; they previously omitted the bounds
  entirely.
- **A stop on an already-`Ended` session reports what was observed.** The early
  return in `ProductionSession::stop` synthesized a *clean* closure whenever the
  session had already reached `Ended` without this call driving the shutdown —
  claiming evidence was closed cleanly without checking. That is the same defect
  class as a status surface reporting healthy capture over a terminal writer, so
  it is fixed here rather than deferred. `ended_stop_outcome` now consults the
  capture coordinator's observed statuses, which outlive the state transition,
  and reuses `shutdown::stop_outcome` so the degraded shape and the
  recoverability→guidance mapping stay single-sourced. The outcome is memoized in
  `stop_result`, so repeat calls stay stable. With nothing observed, the clean
  cleanup note remains the truthful answer.
- **Subscriber lag carries structured recovery.** `SubscriberLag::error()` now
  sets `RetryAdvice::AfterRecovery` and a recovery field ("call list_pages to
  re-read current target state; missed revisions cannot be replayed") rather than
  burying the guidance in prose.

## Implementation Units

- `crates/krometrail-mcp/src/response.rs`
  - `apply_capture_health` + `response_target_id`; `into_call_tool_result` takes
    `&[TargetCaptureStatus]`; `add_capture_warnings` deleted.
  - `RetainedBounds` and `RetainedBounds::project`; `project_retained_bounds`
    rewrites the serialized `retention` object for the full tier;
    `ConciseRetentionStatus.retained_bounds` for the other tiers.
- `crates/krometrail-mcp/src/registry.rs`
  - `capture_health(&BrowserSessionOwner)` helper; all nine response exits pass
    health; `call_lifecycle` reads pre-transition health for `stop`.
- `crates/krometrail-cdp/src/targets/supervisor.rs`
  - `SubscriberLag::error()` gains retry advice and a recovery field.
- `crates/krometrail-cdp/src/session/mod.rs`
  - `ended_stop_outcome(ownership, &[TargetCaptureStatus])` replaces the
    unconditional clean synthesis in the `Ended` early return; the outcome is
    memoized in `stop_result`. Taking the statuses as an argument rather than
    reaching into `SessionShared` keeps the decision a pure function and directly
    testable.

## Testing

- `server::tests::no_tool_reports_healthy_capture_while_the_writer_is_terminal`
  drives `browser_status` at all three detail tiers plus the `start_browser` /
  `attach_browser` lifecycle mapping through the shared exit with a
  `WriterTerminal` capture failure, and asserts every one comes back `degraded`
  with a `capture_failed` warning carrying the restart recovery path.
- `server::tests::retained_bounds_declare_whether_their_endpoints_are_comparable`
  uses the exact observed cross-session pair and asserts the raw fields are gone,
  `comparable_scope` is false, and `span_nanos` is absent — then that a
  same-session pair reports `comparable_scope: true` and `span_nanos: 30`.
- `session::tests::stop_on_an_ended_session_reports_the_observed_terminal_
  outcome_not_a_clean_one` covers all three arms: a terminal writer yields a
  degraded outcome with the restart recovery, a reusable writer keeps its weaker
  guidance rather than being escalated, and no observed failure still yields the
  clean cleanup note.
- Existing capture-warning tests still pass unchanged, which is the evidence that
  moving emission to the shared exit preserved the control-surface behavior that
  was already correct.

## Risks

- **`stop()` idempotency was already solved.** `ProductionSession::stop`
  memoizes its outcome in `shared.stop_result`, and `BrowserSessionOwner::stop`
  removes the session under a mutex before calling it, so a second `stop_browser`
  never reaches the port. No change was needed for idempotency itself; the
  `Ended` early return that *did* report untruthfully is fixed above.
- **The `oldest > newest` root cause lives outside this feature's file
  ownership** and is routed to the agent owning `krometrail-store`.
  `retained_bounds` in `crates/krometrail-store/src/index/retention.rs` orders by
  `rowid ASC/DESC`, i.e. insertion order across all sessions, so the endpoints
  are not a time range at all. The MCP projection's refusal to imply
  comparability stands on its own merit and remains correct after that query is
  fixed, because endpoints drawn from different sessions are genuinely not
  orderable.
- **Capture health costs one extra port read per tool call.**
  `capture_statuses()` is a synchronous in-memory read behind the session mutex,
  so the cost is a lock acquisition, not I/O.
