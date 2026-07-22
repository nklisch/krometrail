---
id: feature-native-control-actionability-upload-affordance
kind: story
stage: done
tags: [browser, agent-ux]
parent: feature-native-control-actionability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Upload affordance resolves to its associated native file input

Design checkpoint for Units 1, 2, and the upload slice of Unit 4 in the parent
feature body (`feature-native-control-actionability`).

## Scope

- Requirement-aware resolution policy: `ReferenceRequirement::FileInput` skips the
  visibility and box-geometry requirements (`resolve_backend_node` /
  `validate_node_state` in `crates/krometrail-cdp/src/control/snapshot.rs`);
  `ResolvedNode.document_quad` becomes `Option<[f64; 8]>` behind a checked
  `geometry()` accessor, and `ResolvedTarget::Element.viewport_point` becomes
  optional with `point()` failing explicitly when geometry was not acquired.
- Associated-file-input canonicalization on `FileInput` kind-requirement miss:
  bounded ordered probe (self, label association, contained input,
  aria-controls/aria-owns, aria-labelledby back-reference, unique
  parent-descendant input), `throwOnSideEffect:false`, one re-validation of the
  canonical node, guided `reference_not_actionable` failure naming the searched
  associations and the required target when no unique association exists.
- Fixture upload patterns (wrapping label, sibling-hidden, unassociated decoy) in
  `tests/fixtures/browser/verified-interactions/index.html`, qualification tests in
  `crates/krometrail-cdp/tests/verified_interactions.rs`, and the SPEC Interaction
  sentence for upload-affordance resolution.

## Acceptance evidence

- Deterministic scripted-CDP tests: canonicalization call sequence ending in
  `DOM.setFileInputFiles` against the associated input's backend node id; hidden
  file input resolves with no `DOM.getBoxModel` call; no-association and ambiguity
  guided failures; pointer requirements unchanged for hidden/zero-area nodes.
- Real-browser qualification: both affordance patterns upload successfully
  (`files.length` observed via evaluate); decoy fails with the guided message.
- `docs/SPEC.md` updated; `bun run docs:build` regenerates the public doc.

## Implementation

- Made `ReferenceRequirement::FileInput` backend-node-only: hidden or zero-area
  file inputs skip visibility/box-model acquisition while connectedness and
  disabled-state validation remain enforced. `ResolvedNode` now carries an
  optional quad behind `geometry()`, and interaction targets carry an optional
  pointer point; pointer and screenshot consumers use the checked geometry path.
- Added the bounded ordered association probe (label, contained input,
  `aria-controls`/`aria-owns`, `aria-labelledby`, unique parent descendant),
  with side-effect analysis disabled, object-to-backend description, one
  canonical-node revalidation, and the specified guided error/recovery.
- Added deterministic scripted-CDP coverage for probe ordering, hidden-input
  no-geometry resolution, and the no-association error. Added the wrapping-label,
  sibling-hidden, and ambiguous decoy fixture patterns and a real-Chrome test;
  the opt-in qualification passed for both uploads and the guided decoy failure.
- Mechanical adaptation: current `ResolvedNode` already had `facts:
  NodeStateFacts` and the post-action re-probe path. The optional geometry was
  added without removing facts, and upload/temporal canonical targets therefore
  retain pre/post fact capture on the existing path. The dependent temporal
  input metadata scaffolding shares this struct change and is completed by the
  next story.
- `bun run docs:build` passed. The full Rust gate was run before this story
  commit and passed.
