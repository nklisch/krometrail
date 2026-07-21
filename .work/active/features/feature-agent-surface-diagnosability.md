---
id: feature-agent-surface-diagnosability
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Agent surface diagnosability

## Brief

Four frictions found during the eighth shakedown against released v1.3.0. Each one cost a
wasted round trip or actively sent diagnosis in the wrong direction. They cluster because
they are one failure repeated: the surface declines to say something it already knows.
Krometrail tracks dialogs but does not report them; the batch route has a correlation id
and an error code but drops both on hard failure; `query_page` knows a `contains` retry
would match but returns a bare `no_match`; `list_frames` holds frame identity and hashes
it away.

Fixing these is additive to the response contract, not a change of shape. Detail level,
action outcomes, and interaction identity are untouched.

## Strategic decisions

- **Release shape**: ships in **1.4.0**, after the 1.3.1 storage hotfix
  (`feature-instance-ownership-lifetime`). No dependency between the two — they touch
  disjoint code — but the storage fix is urgent and this is not, so it must not be gated
  behind this work.
- **Frame identity**: expose **both** the frame `name` and, when a frame shares the main
  document's origin, its real path. Name alone is insufficient because real pages
  routinely leave frames unnamed; path alone loses the author's own label. This relaxes a
  redaction rule for same-origin frames only, so it needs a `docs/SPEC.md` line stating
  the boundary — origin-preserved hashing on third-party network events is unchanged and
  stays as-is. The relaxation is defensible because the agent can already read that
  frame's full content via `snapshot_page` and its full URL via `evaluate_page`; the
  redaction was costing targeting and buying nothing.

## Simplification opportunity

The dialog work should add *one* piece of reported state consumed at three sites (blocked
observation, `handle_dialog`, `browser_status`) rather than three independent special
cases. If the three sites each grow their own dialog check, the design is wrong.

The batch hard-failure path should reuse the projection the degraded path already builds —
that path already returns per-step results, warnings, and a correlation id correctly. This
is plausibly deletion of a separate failure path rather than addition to it.

---

## Findings (from the eighth shakedown, v1.3.0)

Four agent-surface frictions found during the eighth shakedown against released v1.3.0.
Each one cost a wasted round trip or sent diagnosis in the wrong direction. All are
independently small; they cluster because they are the same failure of the surface to say
what it already knows.

## 1. Dialog state is invisible, and the guidance actively misdirects

An open modal JS dialog blocks renderer observation. Krometrail reports:

```
code: page_observation_failed
message: "browser rejected or could not complete the page observation command"
recovery: "retry once; if it fails again, inspect browser compatibility and status"
```

Both halves of that recovery are wrong for the actual cause. Retrying never succeeds while
the dialog is open, and browser compatibility is irrelevant. Reproduced by clicking
`jsPrompt()` on `the-internet.herokuapp.com/javascript_alerts`, then observing. Calling
`handle_dialog` recovered it instantly.

The same blindness from the other side: `handle_dialog` with no dialog open returns only
`"handle_dialog failed: browser rejected the dialog operation"` — no structured code, no
statement that no dialog is present, no correlation id.

Also observed in the log during the same window: `capture.geometry_refresh.pending` retried
5 times against the blocked renderer.

Krometrail already tracks dialogs — it has a tool for them. Modeling "a dialog is open" as
reported state fixes all three sites at once: the blocked-observation error code and
recovery, the `handle_dialog` no-dialog error, and `browser_status`.

## 2. `batch` failures return nothing

A failing batch returned the bare string `batch failed` — no step index, no error code, no
correlation id, no partial step results. The diagnostics log had it the whole time:

```
route: batch, error_code: not_found, correlation_id: 471214f9-...
```

The information exists and is simply not propagated to the caller. Worse, the failure was
intermittent (the identical batch succeeded on retry), and an intermittent failure with no
correlation id is close to undiagnosable. Likely underlying cause: selector resolution
racing a just-completed navigation inside the batch.

Note that batch's *degraded* path is already good — it returns per-step results, warnings,
and a correlation id. Only the hard-failure path drops everything.

## 3. Exact-match `query_page` gives no signal when a near-match exists

On GitHub, `{kind: role, role: link, name: {value: "Cargo.toml", mode: "exact"}}` returns
`no_match`, because the accessible name is `"Cargo.toml, (File)"`. Sites decorate
accessible names routinely, so the natural first query fails on a large fraction of real
pages, and `no_match` gives the agent nothing to act on.

A `no_match` that reported how many candidates would have matched under `contains` would
turn a guess-and-retry loop into one informed follow-up.

## 4. `list_frames` hashes same-origin frame paths, which costs targeting and buys nothing

Every frame URL path is reported as `path_sha256`. On a nested-frame page all frames share
one origin, so there is no way to tell left from middle from right from top — frames can
only be identified by probing each one.

The redaction protects nothing here: `evaluate_page` returns the full frame URLs in a
single call, to the same agent, in the same session, over the same tool surface. Verified
directly.

Note this is specifically about *frame identity*, not a request to weaken URL redaction
generally — origin-preserved hashing on third-party network events is fine and useful.
The cleanest fix is probably additive and avoids the redaction question entirely: expose
the frame's `name` attribute (CDP `Page.FrameTree` carries it), which is what the page
author called it and is exactly the targeting key that is missing.
