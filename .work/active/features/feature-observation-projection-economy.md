---
id: feature-observation-projection-economy
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Lean routine responses and viewport-anchored inspection that survives big pages

## Brief

Three observation-surface improvements from the 2026-07-19 shakedowns:

1. **Scroll/viewport geometry pass must degrade, not fail** (bug, found on
   Wikipedia at 8362 DOM-snapshot nodes): every `scroll` and `set_viewport` on a
   page above the 5000-node cap returns `status: degraded` with the entire
   post-action snapshot unavailable — the geometry decode in
   `control/snapshot.rs` (~line 846) hard-errors past `MAX_SNAPSHOT_NODES`,
   while the accessibility path (~line 514) handles the same pressure by
   omitting and reporting. A standalone `snapshot_page` on the same page
   succeeds. Fall back to the plain accessibility projection (no `document_rect`
   anchoring) with an explicit anchoring-omitted note — bounded-loss accounting,
   operation stays `succeeded`. Fix the query-oriented recovery text on a
   scroll while there.
2. **Trim the routine-success payload.** Concise action responses embed the full
   `record` block (sanitized echo of the caller's own parameters), three-stamp
   timing objects, and repeated context envelopes — a plain click is 2–4 KB and
   clicks/fills dominate call volume. Keep interaction id, outcome, and the
   observation in concise; move the record/provenance echo to expanded. Detail
   must not change action outcome (canonical-result-projection).
3. **Viewport-anchored explicit inspection.** Post-scroll viewport ranking
   exists only inside the scroll response. Expose an explicit anchor option on
   `snapshot_page` (and `query_page` if it falls out naturally) so an agent can
   ask "what is actionable on screen right now" directly — the natural question
   after any scroll, and the workaround for pages where the inline pass hits
   the node cap.

Absorbed backlog: `idea-scroll-geometry-node-cap`. Implementation via peeragent
Codex `gpt-5.6-luna` per operator decision (2026-07-19).

## Simplification opportunity

Unit 1's fallback may let the scroll-path geometry acquisition reuse the
accessibility acquisition's existing bounded-omission machinery instead of
carrying its own hard cap. Unit 2 removes payload rather than adding a new
detail tier if concise can simply shed the record echo.
