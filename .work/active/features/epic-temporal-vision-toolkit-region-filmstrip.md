---
id: epic-temporal-vision-toolkit-region-filmstrip
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

# Region Filmstrip

## Brief

This feature renders a region filmstrip that presents one visual region across time at a readable, consistent scale.

A region can be fixed in viewport coordinates, fixed in source-image coordinates, or supplied independently for each frame by a declared tracking method. A fixed region does not follow a logical element; tracked regions are inferred and must state their tracking method and confidence. The artifact includes a locator image showing the region within a full source frame.

Filmstrip crops use a consistent output scale. Padding is explicit when a region extends beyond a frame. Each crop is labeled with its session-relative timestamp and offset from the query anchor. The output records the region definition, scale, and any per-frame tracking method in provenance.

This feature does not track logical elements unless the caller supplies a tracking method and per-frame region. It focuses on cropping, scaling, and arranging region crops into a deterministic strip.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — useful for localized defects and progressive detail

## Simplification opportunity

- Support fixed viewport and source-image regions first; defer caller-supplied tracking to a follow-up that can prove the contract with evaluation data.
- Render the locator and strip into one image rather than producing separate outputs.
- Reuse the normalization feature for pixel access and scale rather than adding a second decode path.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Region Filmstrip, Normalization, Provenance
- `docs/EVALUATION.md` — Region-filmstrip evaluation criteria

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
