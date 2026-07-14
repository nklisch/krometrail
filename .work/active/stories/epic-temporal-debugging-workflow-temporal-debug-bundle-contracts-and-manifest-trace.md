---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-contracts-and-manifest-trace
kind: story
stage: done
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define Bundle Contracts and Trace Visual Decisions

## Checkpoint

Establish the one-request temporal debug-bundle boundary, complete `ResolvedRange` with the exact resolver-selected semantic anchor, and retain storyboard selection/measurement decisions in the existing temporal-vision manifest so generated and cached artifacts support identical focus correlation. This extends existing authorities; it does not create a resolved-input bundle API, image algorithm, manifest copy, MCP schema, or provenance family.

## Files

- `crates/krometrail-core/src/debug_bundle.rs` (new)
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/krometrail-core/src/timeline/{range.rs,query.rs,mod.rs}`
- `crates/temporal-vision/src/{select.rs,render.rs,provenance.rs,lib.rs}`
- `crates/temporal-vision/tests/{storyboard.rs,contracts.rs}`

## Contract

- Add `ResolvedAnchorReference` and `ResolvedAnchor { reference, requested_time, effective_time }` to the existing `ResolvedRange`. Explicit/wall-clock/source-frame intervals use their documented midpoint; interaction/latest use exact dispatch and resolved `InteractionId`; navigation/marker use exact observation time/ID; final resolution clamps only to retained bounds and reports both times.
- Add `TemporalDebugBundleRequest`, context, exact requested/resolved/effective result types, component availability/degradation/warning types, and object-safe `TemporalDebugBundles::bundle`. The only request owns `TemporalQueryRequest`; no method accepts `ResolvedRange`.
- Evolve `StoryboardSelection` with `StoryboardVisualSummary` containing first change, peak baseline change, and peak adjacent changed-area moments from existing comparisons. Add the validated selection as optional typed data on the existing generic `ArtifactManifest`.
- Bump storyboard/orientation descriptor to `1.1.0`; every storyboard/orientation manifest requires a valid trace. The project is prepublic, so trace-less unpublished manifests receive no compatibility path and regenerate from retained sources. Other generator versions and PNG rendering remain unchanged.

## Acceptance evidence

- All seven temporal anchors produce one exact typed requested/effective anchor, including latest-interaction identity and partial-retention clamping, with validated Serde and no second lookup contract.
- Bundle request/result values revalidate nested query/marker bounds and expose exact existing evidence contracts without bytes, paths, URIs, or copied artifact/context types.
- First/peak/adjacent visual summaries reuse selector measurements, never cross declared gaps, preserve exact source IDs/indexes/timestamps/outcomes, and use deterministic ties.
- Storyboard and orientation manifests validate trace/source/selected/role agreement; difference-map/filmstrip/motion manifests reject a storyboard trace.
- Descriptor `1.1.0` changes storyboard/orientation cache identity, leaves other kinds unchanged, rejects trace-less storyboard/orientation manifests, and preserves existing storyboard/orientation PNG golden bytes.

## Ordering

This is the first checkpoint. It unblocks policy/focus code because cached artifact manifests must expose authoritative visual decisions before the bundle can correlate events. On green verification this child advances directly to `done`; only the parent feature receives standard review.

## Implementation notes

- Execution capability: highest-capability cohesive inline ownership, as required by the caller; direct reads covered the bounded core and temporal-vision surfaces without nested agents.
- Review weight: standard from the caller; not applicable at this checkpoint because it is a child story and advances directly to done after verification.
- Files changed: `crates/krometrail-core/src/debug_bundle.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-core/src/timeline/{mod.rs,range.rs}`, `crates/temporal-vision/src/{lib.rs,provenance.rs,render.rs,select.rs}`, and `crates/temporal-vision/tests/{contracts.rs,storyboard.rs}`.
- Tests added: exact seven-kind resolved-anchor identity/time/Serde and partial-retention clamping; bundle request nesting, marker bounds, privacy, and unknown-field rejection; storyboard first/peak/adjacent trace identity, gap exclusion, deterministic ties, current/old manifest compatibility, kind/source/role validation, descriptor isolation, and unchanged PNG SHA-256.
- Simplification: interaction and latest-interaction seed construction share one exact helper; typed storyboard decisions replace any need to parse free-form parameters or remeasure cached evidence; existing artifact/context/range/error contracts remain the bundle result authorities.
- Discrepancies from design: the manifest stores its private optional trace and algorithm descriptor behind boxes to keep existing public artifact enums Clippy-clean after the trace expansion; getters, constructors, and serialized shape remain the designed contracts. No requested behavior changed.
- Adjacent issues parked: none.

## Verification

- Rust 1.85: `cargo fmt --all -- --check` passed through `rustup run 1.85.0`.
- Rust 1.85 core + temporal-vision: all-target `check`, `test` (101 core and 51 temporal-vision tests), and Clippy with `-D warnings` passed.
- Rust 1.85 workspace: `cargo check --workspace --all-targets --locked` passed.
- Focused integration: all seven store-backed temporal query forms passed (3 tests), and cache fingerprint/version isolation passed (4 root cache tests).
- Storyboard/orientation PNG golden remained `b606148fe214fd4d68545e1ad3379299f427a8c75e1797f7f7dd34358b1d2417`.
