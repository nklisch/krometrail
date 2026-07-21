---
id: feature-capture-health-surfacing
kind: feature
stage: drafting
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
