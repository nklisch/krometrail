---
id: feature-visibility-emission-ordering
kind: feature
stage: drafting
tags: [browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Visibility fence emission-side ordering

## Brief

Cross-model review finding (minor, adjudicated as parkable) on
`feature-window-lifecycle-integrity`: the visibility ordering fence stamps
observed session time when the screencast visibility event is dequeued from the
transport subscription (`capture/pipeline.rs` visibility reader →
`SessionCaptureObserver::visibility_changed`), not when Chrome emitted it. A
hidden event sitting in the subscription channel while an activation write-back
commits gets a post-activation stamp and passes the fence — the original
overwrite race survives, confined to the transport-queue window (small, and
self-healing via the running screencast re-emitting Visible). Chrome supplies
no timestamp on `Page.screencastVisibilityChanged`, so closing it fully needs
an emission-side ordering token: stamp at the transport event pump before
fan-out, or sequence all visibility-bearing transport events through one
ordered path.

Also in scope: producers use `session_time().unwrap_or(SessionTime::ZERO)`,
which would permanently fence a producer whose clock normalization fails (only
possible pre-origin) — prefer dropping the observation with a diagnostic over
stamping zero.

## Simplification opportunity

If stamping moves to the transport event pump, the dequeue-side stamping in the
visibility reader becomes dead and should be removed rather than kept as a
fallback.

Origin: `.work/backlog/idea-visibility-fence-emission-stamp.md`.
