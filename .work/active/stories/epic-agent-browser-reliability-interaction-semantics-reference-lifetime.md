---
id: epic-agent-browser-reliability-interaction-semantics-reference-lifetime
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-interaction-semantics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Preserve references across harmless observations

## Checkpoint

Replace observation-scoped active snapshots with a bounded latest-document registry whose
generation changes only at attachment/document boundaries and whose node identifiers remain tied
to backing backend nodes. This checkpoint owns the reference-lifetime portion of GitHub issue #11.

## Acceptance evidence

- [ ] A still-present reference from one snapshot resolves after later snapshot/live-observation
      calls in the same attachment and document.
- [ ] AX-tree reordering cannot retarget a reference, and memory remains bounded by the latest full
      tree per live target rather than snapshot count.
- [ ] Navigation, document replacement, reconnect, target closure, and backing-node detachment
      continue returning structured stale-reference failures with recovery.
- [ ] `docs/SPEC.md` and `docs/ARCHITECTURE.md` replace the now-false fresh-snapshot invalidation
      assertion in place.

## Ordering and blocker

Independent of input-contract work. Its corrected registry is prerequisite to pointer preparation
because scroll-triggered re-resolution must preserve identity without relying on the newest
observation generation.
