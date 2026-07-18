---
id: epic-temporal-video-artifacts-retained-generation-bounded-generation-service
kind: story
stage: implementing
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts-retained-generation
depends_on: [epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bounded retained temporal-video generation service

## Design checkpoint

Implement the resolved-range application service that partitions exact retained sources into visual epochs, derives bounded geometry and optional versioned meaningful-frame selection, builds the canonical plans, creates source/gap encoder inputs, serializes equal cache keys through one transient lock, invokes the injected encoder, rejects contradictory output, and publishes complete ordered clip results through the shared artifact store.

## Acceptance evidence

- Pure tests prove exact geometry fitting, deterministic visible meaningful-frame selection, gap-slate rendering, source/gap input ordering, and cache sensitivity.
- A deterministic fake encoder proves exact plan/profile/frame composition, cache reuse and equal-key single encoding, encoder identity/profile/hash rejection, multi-epoch all-or-error results, and no FFmpeg/process dependency.
- Cancellation/deadline tests cover source load, scheduler/lock wait, selection, encode, and pre-publication boundaries without publishing a partial clip.

## Ordering constraints

- Depends on `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence`.
- The lifecycle qualification checkpoint exercises this service against the real store and may tighten implementation, but may not introduce an alternate cache, range, plan, or deletion authority.

## Execution contract

- Worker capability: highest available, selected by active autopilot because exact cache/provenance and external-work cancellation are high consequence.
- Review weight: `standard` from autopilot default; this child closes on green evidence and the integrated feature receives the single independent review pass.
