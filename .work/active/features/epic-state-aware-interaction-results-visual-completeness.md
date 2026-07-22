---
id: epic-state-aware-interaction-results-visual-completeness
kind: feature
stage: drafting
tags: [agent-ux, browser, visual]
parent: epic-state-aware-interaction-results
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Visual completeness marker

## Brief

Issue #14 finding #5: an immediate post-action image contained compositor
artifacts (overlapping duplicate cards); the retained temporal evidence (60
gap-free frames) disproved the apparent product defect. Callers need to know
when the immediate image cannot be trusted as settled so they consult retained
evidence before reporting a visual defect.

The signal already exists and is discarded: the post-action double
requestAnimationFrame compositor wait (bounded 250 ms) logs
`browser.compositor.signal_unavailable` via tracing when the signal never
arrives, then captures anyway with no marker on the response. This feature
surfaces that observed state as a bounded visual-completeness marker on the
immediate screenshot — the `EncodedScreenshot` warning surface is the existing
attach point (its only producer today is the tall-screenshot guidance). The
marker states that visual completeness is unconfirmed and points at retained
evidence; it does not judge the pixels.

Does NOT cover: pixel-content analysis of immediate captures (dark-frame or
damage detection). Frame analysis remains the temporal-vision pipeline's
authority; v1 surfaces only what the compositor-readiness path already
observes. If design finds the rAF signal insufficient to explain the reported
artifact class, it may add a bounded, cheap readiness fact — but never a
pixel-analysis pass on the immediate path.

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: independent capability — no dependency on the
  postcondition block; can land in parallel with `postcondition-core`.

## Simplification opportunity

- Replace the tracing-only discard with the surfaced marker — one signal, one
  consumer path; no parallel "compositor health" side-channel.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (visual-completeness marker;
  two-rAF bounded wait)
- `docs/VISUAL-EVIDENCE.md` — evidence classes (immediate vs retained)
- GitHub issue #14, finding #5 (`44b8a67f-e2bb-49eb-9526-5af6bfd745c7`)
