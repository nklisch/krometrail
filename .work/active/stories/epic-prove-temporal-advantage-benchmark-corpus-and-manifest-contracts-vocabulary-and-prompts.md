---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-vocabulary-and-prompts
kind: story
stage: done
tags: [testing, visual]
parent: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-corpus]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Commit benchmark matrix, evidence conditions, and task prompts

## Checkpoint

Define the exact matrix consumed by later capture, artifact, scoring, and manual-agent
features. This story only defines inputs and vocabulary; it does not start Chrome, generate
artifacts, invoke a model, or score a run.

## Matrix contract

Extend `docs/evidence/temporal-evaluation/v1/benchmark-definition.json` with one canonical
matrix. Its order is explicit and part of the digest:

1. capture rows are ordered by family, then case ID, then duration `[16,33,50,100,200]`, then
   repetition `0..29`;
2. interpretation/debugging rows use the same declared cases and conditions, then a deterministic
   Fisher-Yates permutation driven by seed `0x4b524f4d45545241`; the realized ordered trial IDs
   are stored in the manifest;
3. no process, host, hash-map, or filesystem enumeration may affect order;
4. the corpus definition, seed, duration list, repetition minima, and realized order are hashed
   into the run input identity.

Capture qualification requires 30 repetitions per case/duration/configuration. Model
interpretation requires at least 10 randomized scenario evaluations per required family and
condition before a threshold can be decisive. A partial row remains `inconclusive`; it is not
rounded up or discarded. The required model families are movement, flicker, and layout, with
stable controls included in every interpretation comparison. DOM-opaque cases remain an
additional named family and never silently substitute for the required three.

## Evidence conditions

The definition contains exactly these condition IDs, all using one captured source interval:

| ID | Input | Selection/retrieval rule |
| --- | --- | --- |
| `A-final-screenshot` | Final post-action screenshot and ordinary current page observation | One final image; no historical source retrieval. |
| `B-uniform-storyboard` | Uniform source-frame storyboard | Exactly 8 source-frame slots, selected by normalized interval position; fewer retained frames are an explicit incomplete-input state. |
| `C-change-aware-storyboard` | Change-aware source-frame storyboard | At most 8 source-frame tiles, using the committed `temporal-storyboard` algorithm descriptor and its provenance; no additional retrieval. |
| `D-temporal-bundle` | Before/during/after composite, change-aware storyboard, difference map, capture summary, and evidence references | Same source interval and no more than 8 source-frame tiles; the bundle cannot hide gaps or turn references into availability. |
| `E-progressive-source` | Condition D plus source-frame and region-filmstrip retrieval | Starts with D, permits at most two retrieval requests of four source frames each and one declared region filmstrip; returned/evicted/missing IDs are recorded. |

Condition definitions include `condition_id`, `source_interval_policy`, artifact kind and
algorithm/version, initial tile limit, retrieval budget, allowed inputs, prompt ID, and scoring
rule IDs. The D-vs-B/C tile comparison counts source-frame tiles, not labels or textual
references. Condition E is a workflow condition and its retrievals are scored separately.

## Structured task prompts

Store the exact prompt templates in the committed definition with stable IDs and versions. The
interpretation prompt requires a bounded JSON answer with these fields:

```text
observation_answer {
  temporary_state: yes | no | uncertain,
  state_order: [baseline | changed | final | intentional_motion | unknown],
  affected_region: { x, y, width, height } | unknown,
  motion_behavior: monotonic | reversal | teleport | flicker | layout_shift | none | uncertain,
  judgment: defective | intentional | uncertain,
  uncertainty_reasons: [capture_gap | missing_source | insufficient_detail | other],
  evidence_refs: [opaque evidence handle]
}
```

The system prompt fixes the non-diagnostic posture: visual measurements do not establish cause,
ground truth, or defect status; an answer must use `uncertain` when a gap or missing source
prevents the claim. The task prompt asks the questions in the order above without naming the
fixture family, case ID, variant, or expected answer. The debugging prompt is a separate exact
version and asks for reproduce, diagnose, focused patch, and final-state plus temporal
verification; it is not used in the interpretation score.

Scoring dimensions are a single registry in the definition: `transient_defect_identification`,
`state_order`, `affected_region`, `motion_behavior`, `gap_uncertainty`, and
`stable_control_false_positive`. Each dimension declares its allowed result vocabulary, whether
it requires ground truth, and whether it contributes to the thesis threshold. No later scorer
may invent a spelling or alias for these values.

## Acceptance evidence

- [x] The definition contains all five conditions, exact tile/retrieval budgets, prompt IDs and
      hashes, scoring IDs, seed, fixed duration order, repetition minima, and order policy.
- [x] Canonical matrix generation produces the same ordered capture rows and randomized trial
      order on repeated runs and across platforms.
- [x] Prompt validation rejects unknown answer fields, unbounded strings, invalid enum values,
      missing evidence references, or a claim of certainty with an unresolved gap.
- [x] The definition does not reveal a case label or defect mechanism in the model-facing prompt
      templates or condition metadata.
- [x] The matrix explicitly marks the required family coverage and makes partial, missing, or
      unsupported rows `inconclusive`, `blocked`, or optional `skipped` according to the feature
      status rules rather than treating them as passing controls.

## Implementation notes

- **Execution capability**: inline feature-owning worker with direct-read integration mapping;
  the existing corpus contract was the correct boundary for this cohesive extension.
- **Review weight**: standard, inherited from project default; child stories advance directly to
  `done` after verification and do not enter review.
- **Files changed**: added `matrix.rs`, `conditions.rs`, `prompts.rs`, and `vocabulary.rs`; extended
  `corpus.rs`/`lib.rs`; regenerated the canonical definition and JSON Schema; expanded corpus
  contract tests.
- **Tests added**: deterministic capture and seeded Fisher–Yates trial ordering, exact A–E
  condition budgets, coverage status transitions, one scoring registry, prompt hash/non-leak
  checks, unknown-field answer rejection, bounded answers, and input-identity checks.
- **Simplification**: kept all condition, prompt, scoring, and status variants in typed registries;
  no run manifest, scoring implementation, Chrome/artifact execution, CI output handling, or
  compatibility alias was added.
- **Discrepancies from design**: none; the definition remains the single current prepublic
  contract and the authoritative generator remains the existing Rust binary.
- **Adjacent issues parked**: none.
- **Verification**: Rust 1.85 locked workspace fmt, check, test, and clippy gates pass; the
  definition and schema were regenerated with `generate-benchmark-definition`.

## Ordering

This story depends on the corpus story because every matrix row must resolve to an existing case,
phase, anchor, and fixture digest. The manifest story consumes this exact vocabulary and must not
redeclare conditions, prompts, statuses, or scoring dimensions.
