---
id: idea-visibility-fence-emission-stamp
created: 2026-07-19
updated: 2026-07-19
tags: [browser]
---

Cross-model review finding (minor, adjudicated as parkable) on
`feature-window-lifecycle-integrity`: the visibility ordering fence stamps
observed session time when the screencast visibility event is dequeued from the
transport subscription (`capture/pipeline.rs` visibility reader →
`SessionCaptureObserver::visibility_changed`), not when Chrome emitted it. A
hidden event sitting in the subscription channel while an activation write-back
commits gets a post-activation stamp and passes the fence — the original
overwrite race survives, confined to the transport-queue window (small, and
self-healing via the running screencast re-emitting Visible). Closing it fully
needs an emission-side ordering token (Chrome supplies no timestamp on
`Page.screencastVisibilityChanged`), e.g. stamping at the transport event pump
before fan-out, or sequencing all visibility-bearing transport events through
one ordered path. Also noted: producers use
`session_time().unwrap_or(SessionTime::ZERO)`, which would permanently fence a
producer whose clock normalization fails (only possible pre-origin) — prefer
dropping the observation with a diagnostic over stamping zero.
