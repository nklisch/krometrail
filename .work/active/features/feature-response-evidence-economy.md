---
id: feature-response-evidence-economy
kind: feature
stage: drafting
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Make automatic evidence match what the action changed

## Brief

Refinements on top of `compact-live-observations` (done, 1.0.4) and
`agent-visual-response-surface-visual-defaults` (done), driven by a live shakedown
session. Four gaps remain between what an action returns and what an agent needs:

1. **Unchanged snapshots are re-dumped.** Every state-changing response — including pure
   selection/focus operations like `activate_page`, `select_page`, and `go_back` — embeds
   the full ranked target list (~45 targets on Hacker News) even when the snapshot
   generation did not change from the previous response. Per-target `states` arrays repeat
   near-universal defaults (`focusable: true` on every link). A single click costs
   thousands of tokens of mostly-identical data.
2. **Post-scroll evidence describes the wrong viewport.** After `scroll` to y=2000, the
   returned targets and semantic outcomes still describe the top of the page. An agent
   that scrolls to reveal content learns nothing from the structured response and must
   take a screenshot.
3. **No automatic image where structure is known-stale.** Strategic decision below.
4. **Full-page screenshots of tall pages are model-useless.** A 28,000px article returns
   one 1658x28276 image with no warning; downscaled for model input it is unreadable.

## Strategic decisions

- **Automatic image policy**: Staleness-triggered — keep routine operations image-off, but
  auto-inline one viewport image exactly when the structured projection is known-stale or
  low-information: after `scroll`, viewport changes, and activation. Chosen over
  image-on-every-action (token cost, redundancy) and over pure opt-in (agents stay blind
  after scroll). Preserves the cheapest-sufficient-evidence contract; explicit
  `inline_images` overrides always win.

## Simplification opportunity

Snapshot dedupe can reuse the existing generation identity: when the post-action snapshot
generation equals the previously projected one for the same target, project the identity
and omission counts instead of the target rows. Viewport anchoring can reuse the existing
ranking pass with the current visual viewport as the ranking window rather than adding a
second ranking system. Tall-screenshot handling should prefer guidance plus bounded output
(existing output-limit machinery) over a new tiling subsystem.
