---
id: story-skill-inline-image-default-drift
kind: story
stage: done
tags: [prose, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Reconcile skill/doc inline-image defaults with purpose-sensitive runtime defaults

## Brief

`agent-visual-response-surface-visual-defaults` (done) made image defaults
operation-sensitive: screenshot, live observation, temporal bundle, source-frame fetch,
artifact, and filmstrip routes default to one inline image; routine routes default off.
The shipped plugin skill text (observed at 1.2.0) still teaches the older contract in
places — e.g. the temporal bundle section says the default response has "no inline image
bytes. Add {\"response\":{\"inline_images\":true}} when the primary ... image should be
embedded immediately", and the observed runtime behavior (bundle inlined its storyboard
without any request) contradicts that sentence. Sweep the skill instructions and
foundation docs for remaining old-default prose, align them with the purpose-sensitive
defaults, and regenerate `docs/public/llms-full.txt` via `bun run docs:build`.

If `feature-response-evidence-economy` lands staleness-triggered auto-images, its own
doc pass covers the new surfaces; this story only removes drift about the already-shipped
defaults.

## Acceptance

- No remaining skill/doc sentence claims a no-inline default for a route whose runtime
  default is image-on (and vice versa).
- `docs/public/llms-full.txt` regenerated, not hand-edited.

## Completion notes

The sweep found no remaining inline-image default drift. The temporal bundle sections in
`plugin/skills/krometrail/SKILL.md` and `references/evidence.md` already state that one primary
image is inline by default, and the current SPEC/foundation wording agrees. The remaining
no-image wording applies only to routine post-action routes, whose runtime default is image-off.
No documentation change was needed, so `docs/public/llms-full.txt` was not changed; `bun run
docs:build` was nevertheless run after the story-1 foundation-doc edits and completed successfully.

- Files changed: this story body only (committed with story 2 because no prose drift remained).
- Tests: repository-wide prose sweep; no stale claim matched.
- Stage intentionally remains `implementing` per the implementation request; no other work item
  was advanced.
