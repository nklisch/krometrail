---
id: feature-native-control-actionability-upload-affordance
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: feature-native-control-actionability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
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
