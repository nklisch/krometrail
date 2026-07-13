---
id: epic-temporal-vision-toolkit-motion-history
kind: feature
stage: drafting
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Motion-History Image

## Brief

This feature renders a motion-history image that accumulates recently changed pixels over one spatial reference, with explicit source-vs-inference boundaries.

The default rendering combines a subdued source-frame reference, a motion-history layer showing recent change stronger than older change, and changed-region outlines. A visible decay legend maps intensity to relative time. Start and end labels are included. The artifact links to the storyboard and region filmstrip for disambiguation when overlapping states make text unreadable.

The crate does not add direction arrows, velocity vectors, object trajectories, or other inferred motion analysis to this source-derived artifact. Any future inferred overlay must be labeled as a separate artifact with its own method, version, and confidence.

This feature depends on the same thresholded change measurements used by storyboard and difference-map rendering. It does not produce storyboards or difference maps.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — bounded experiment in source-derived motion visualization

## Simplification opportunity

- Implement one deterministic decay model and expose it through parameters rather than multiple rendering modes.
- Produce a single combined image; do not split the reference, history, and outline layers unless evaluation shows agents need them separately.
- Explicitly forbid inferred overlays in this feature; inferred analysis is a separate future extension with its own provenance contract.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Motion-History Image, Inferred Analysis, Visual-Change Measurements
- `docs/EVALUATION.md` — Motion-history evaluation criteria

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
