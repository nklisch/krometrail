---
id: refactor-centralize-artifact-generator-request-projections
kind: story
stage: done
tags: [refactor, visual, storage]
parent: null
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize artifact generator request projections

## Brief

The bounded artifact adapter repeats the same `ArtifactGeneratorRequest` variant
projection in `src/artifacts/generators.rs`: normalization is matched separately
by `normalization_parameters` (lines 58-69), `normalization_identity`
(lines 71-83), `estimated_normalized_bytes` (lines 86-106), and `normalize`
(lines 108-118); output limits are matched again by `validate_output_limits`
(lines 404-423) and `validate_outputs` (lines 425-448). These matches are exact
variant-to-field policy duplication in the artifact implementation introduced by
`13e6464` and present in the final artifact tree at `622f9be`.

Extract private, exhaustive request-projection helpers in the generator module and
reuse them at those call sites. Keep the public core request types unchanged and
keep region-filmstrip's intentional lack of a normalization request explicit.

**Source lens**: elimination first / exact duplication

**Rationale**: one private projection path makes the four generator variants' shared
normalization and output-limit policy easier to audit and prevents the repeated
matches from drifting while removing no contract or behavior.

**Black-box classification**: pure refactor. Generator selection, effective
normalization, cache parameter bytes, output ceilings, error messages, output
ordering, manifests, and serialized request shapes remain unchanged.

## Acceptance criteria

- [ ] One private helper owns the `ArtifactGeneratorRequest` to normalization
  projection used by all normalization planning, identity, estimation, and
  execution paths.
- [ ] One private helper owns the `ArtifactGeneratorRequest` to output-limit
  projection used by both preflight and post-generation validation.
- [ ] Region filmstrip remains the explicit no-normalization case, and all existing
  error behavior and limit comparisons remain unchanged.
- [ ] No public API, schema, cache transcript, manifest, decoder, or retention
  behavior changes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes

- **Evidence**: `13e6464` added the adapter projections; final commit-tree
  verification uses `622f9be:src/artifacts/generators.rs:58-118,404-448`.
- **Files**: `src/artifacts/generators.rs`; this story file.
- **Tests**: retain the existing service, limit, normalization, cache-identity,
  and qualification coverage; no new test is needed for a private match
  extraction unless verification exposes a missing important boundary.
- **Ordering**: this story depends on the artifact feature reaching its review
  decision before the refactor is implemented. It does not overlap the active
  browser-event or progressive-evidence design files.

## Implementation record

- Execution capability: baseline inline ownership; one private exhaustive-projection extraction in one module.
- Added `normalization_request` and `output_limits` as the sole immutable projections for shared generator fields. Region filmstrip remains an explicit `None` normalization case.
- Normalization planning, identity serialization, byte estimation, and execution now consume the same projection. Output preflight and generated-output validation consume the same limit projection.
- Public request types, mutable effective-scale materialization, generator dispatch, cache parameters, errors, comparisons, manifests, and output order are unchanged.
- Rust 1.85 locked format, full workspace all-target tests, and Clippy with warnings denied passed.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Evidence**: Bounded standalone-story review inspected commit `ee4d152`, confirmed the helper matches remain exhaustive, each caller receives the same copied validated field as before, region filmstrip stays normalization-free, and mutable effective-scale materialization remains independent. The full Rust 1.85 workspace gate passed. No independent reviewer ran, as required for a standalone story.

## Risk and rollback

**Risk**: Low. The candidate consists of exhaustive private matches with no
observable side effects; the main risk is accidentally changing which callers see
`None` or which output field is validated.

**Rollback**: Revert the implementation commit to restore the original local
matches. The artifact feature and its public contracts remain untouched.
