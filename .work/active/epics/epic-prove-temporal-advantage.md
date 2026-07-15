---
id: epic-prove-temporal-advantage
kind: epic
stage: implementing
tags: [testing, visual, browser]
parent: null
depends_on: [epic-temporal-debugging-workflow]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-15
---

# Prove the Temporal Advantage

## Brief

This epic establishes whether Krometrail’s product thesis is true. It delivers the deterministic defect corpus, stable controls, capture-duration sweeps, artifact interpretation comparisons, agent debugging scenarios, browser-control reliability checks, retention validation, performance measurements, and platform evidence defined by the evaluation foundation.

The evidence program compares final screenshots, uniform sampling, change-aware storyboards, temporal bundles, and progressive source access. It records model, browser, operating-system, prompt, artifact, capture, and scoring details so improvements are reproducible rather than anecdotal.

This is not a generic test-cleanup container. Its output is the project’s defensible claim boundary: which transient durations are captured, which artifacts help which evaluated agent, where false interpretations occur, and whether an agent debugs more successfully with temporal evidence. The completed prerequisite `epic-temporal-debugging-workflow` is archived at `v0.2.20` with approval ref `d9a1b56`; this epic evaluates those delivered capabilities rather than redesigning them.

## Foundation references

- `docs/agents.md` — authoritative foundation navigation and current-runtime boundary
- `docs/VISION.md` — Product Thesis, Core Experience, Local-First Operation, and Success
- `docs/SPEC.md` — Supported Environment, Continuous Visual Capture, Temporal Queries, and Exclusions
- `docs/ARCHITECTURE.md` — Temporal Visual Crate, Capture Tasks, Artifact Generation, Retention, and Observability
- `docs/VISUAL-EVIDENCE.md` — Evidence Classes, Provenance, Capture Gaps, Progressive Detail, and Non-Diagnostic Posture
- `docs/EVALUATION.md` — complete benchmark, scoring, threshold, platform, model, and reproducibility contract
- `tests/fixtures/browser/README.md` — standalone target-application boundary
- `docs/evidence/cdp-transport/v2/README.md` — completed transport qualification boundary
- `docs/evidence/cross-platform-smoke/v1/README.md` — completed production capture-smoke boundary and honest absent high-DPI evidence

## Design decisions

- **Qualification is layered, not one green check.** Deterministic CI-safe harnesses establish fixture, artifact, manifest, scorer, fake-clock, gap, control-contract, and retention-contract correctness. Opt-in live-Chrome runs establish production capture and system behavior. Manually authorized Codex/multimodal runs establish only the named model-and-prompt result. A layer never silently substitutes for another.
- **Thesis thresholds are reportable, not release blockers.** The capture envelope and agent-effectiveness thresholds produce `pass`, `fail`, or `inconclusive` evaluation outcomes. They do not block a software release and do not permit a release to claim validated improvement when the result is unmet or incomplete.
- **Initial agent scope is Codex-specific.** The initial manual lane uses the locally available Codex CLI because that is the authorized execution surface. A Codex result names its provider/model/version and makes no cross-model or general multimodal claim. Another model family requires its own separately authorized, independently identified run set and comparison.
- **Paid execution is never implicit.** No CI job, Rust test, fixture server, MCP operation, sub-agent, or implementation worker may invoke a paid agent. An operator must explicitly authorize each manual run set and its budget outside this design transition. Absent authorization is `blocked` or `inconclusive`, never a passing no-op.
- **Reference-host evidence is the agent-lane gate.** Manual interpretation requires the exact Linux stable-Chrome reference-host child checkpoint and one declared live evidence manifest. It does not wait for the platform parent, macOS default-DPI, macOS high-DPI, or optional Linux Chromium. MacOS absence leaves cross-platform evidence `inconclusive`; it is never silently claimed and never blocks reference-host agent effectiveness.
- **Prepublic clean design.** Benchmark contracts, schemas, prompts, and evidence handles are new unpublished surfaces. Do not add compatibility aliases, legacy serialized shapes, migrations, fallback providers, or shims for hypothetical consumers. Remove superseded local scaffolding when a later implementation makes it unnecessary.
- **No UI work.** This epic produces browser targets, machine-readable evidence, still artifacts, and agent workflows; it has no human screen, page, modal, or journey. UI mockups do not apply.

## Shared benchmark contract

### Corpus and conditions

The committed corpus contains five named families: movement reversal, flicker, transient layout change, DOM-opaque motion, and stable controls. Each case declares a stable scenario identifier, fixture revision, intended interaction anchor, visible-state identifiers, expected intervals, affected region/geometry where applicable, final stable state, and whether the behavior is defective or intentional. Ground truth is authored by the fixture and is not derived from Krometrail frame measurements.

The duration sweep uses the Evaluation values: approximately one display frame, 33 ms, 50 ms, 100 ms, and 200 ms. Each decisive live capture/configuration row declares its repetition count in the committed benchmark definition; the initial qualification minimum is 30 repetitions per duration/case/configuration. The agent lane uses at least 10 randomized scenario evaluations per required family and condition before a threshold can be decisive. A partial run may be useful exploratory evidence, but remains `inconclusive` for the product-thesis gate.

The five visual conditions are fixed and use the same captured source interval: A final screenshot, B uniformly sampled source frames, C a change-aware storyboard, D the temporal debug bundle, and E the bundle with progressive source-frame/region retrieval. Condition definitions, prompt text, scoring vocabulary, artifact algorithm versions, tile limits, and selection rules are versioned benchmark inputs, not ad hoc run flags.

### Exact reproducibility manifest

Every local run writes one canonical manifest before its outputs are scored. It includes:

- manifest/schema version, benchmark identifier, benchmark-definition Git revision, harness revision, scorer revision, and ordered fixture-file SHA-256 digest;
- case IDs and order, random seed, duration list, repetition count, evidence condition, threshold configuration, and declared run status;
- Krometrail revision, Cargo.lock/Rust toolchain identity, artifact algorithm and adapter versions, capture configuration, viewport, device scale, image format/quality, and retention budget;
- platform, architecture, browser product/version/protocol/revision, and the exact runtime capability/configuration identity;
- ordered source-frame IDs, capture ordinals, normalized/session times, gap and retention state, artifact manifest/output hashes, and source/artifact availability for every accepted visual claim;
- for model runs only: provider, model identifier and dated version/alias, invocation date, system/task prompts, tools, input limits, raw answer, token/image usage where available, score, scoring rationale, and operator authorization reference; and
- explicit `non_claims`, failure/skip/block reasons, and whether the record is complete enough for threshold assessment.

Canonical manifests use deterministic field order, sorted maps, ordered arrays, finite canonical numbers, UTF-8 bounded strings, and SHA-256 hashes. They contain no credentials, private endpoints, local absolute paths, usernames, page bodies, or raw sensitive browser data. A manifest hash covers the exact benchmark inputs and relevant source identities; it is not a substitute for retaining those source frames.

### Committed definitions versus per-run evidence

Git contains benchmark fixtures, hidden-ground-truth definitions, prompts, condition schemas, scoring rules, manifest schemas, canonical samples, harness/scorer source, and README/provenance instructions. Per-run manifests, source frames, generated artifacts, model answers, transcripts, patch workspaces, logs, and aggregate results live under ignored `target/temporal-evaluation/` output directories and are never silently added to Git. The existing repository `target/` ignore rule supplies the default boundary; this design does not edit generated documentation or create a product command for evaluation.

A result can be reproduced only while its manifest-referenced source frames and artifacts remain available. If they are deleted, evicted, corrupted, or never collected, the result says so. A committed schema/sample proves serializer and validation behavior; it is not observed platform or model evidence.

## Execution and evidence gates

| Lane | What it may prove | Authorization and environment | Honest outcome rule |
| --- | --- | --- | --- |
| Deterministic CI | Corpus/schema/condition/scorer determinism, artifact provenance, fake-clock ordering, gap/retention/control contracts, and reproducible scoring | Ordinary locked CI; no browser, network, paid model, or external service | Green proves only the deterministic harness and product-contract tests. It cannot satisfy live capture or agent thresholds. |
| Opt-in live capture/system | Production Chrome/Chromium capture, timing, control, storage, retention, and performance measurements for the declared configuration | Explicit real-browser opt-in, a supported local installation, and the fixture; no paid model | Missing installation, unsupported platform, failed probe, or required observation is `blocked`, `skipped`, or `inconclusive`; never a fake pass. Complete below-threshold data is `fail`. |
| Platform matrix | Comparable Linux/macOS evidence with browser, DPI, fixture, and runtime identity | Required Linux Chrome, macOS Chrome default-DPI, and macOS Chrome high-DPI lanes; Linux Chromium remains best-effort | A single host is not cross-platform evidence. Missing macOS/high-DPI evidence leaves the matrix incomplete. Optional Linux Chromium may be skipped with its reason. |
| Manual multimodal interpretation | Named model's answers to structured observation tasks under conditions A–E, including uncertainty and source tracing | Explicit operator authorization and budget plus one declared Linux stable-Chrome reference-host live evidence run; never automatic | No authorization/model/reference-host source interval/minimum sample yields no decisive result. MacOS/default-DPI/high-DPI absence does not block this lane. The record remains `blocked` or `inconclusive`, not pass. |
| Manual agent debugging | Named model's reproduce–diagnose–patch–verify outcomes against current-state and temporal conditions | Same explicit manual authorization; isolated task workspaces and recorded prompts/fixtures | A successful patch is evidence for the named scenario/model/configuration, not automatic diagnosis, replay, or general model capability. |

The existing CDP transport v2 and cross-platform smoke evidence are consumed as prerequisite context with their own schemas and non-claims. They do not satisfy this epic's duration sweep, full defect corpus, artifact comparison, retention validation, or product-thesis thresholds. In particular, the currently absent macOS high-DPI smoke artifact remains absent until a future run observes the required scale; the evaluation lane must not manufacture one from wrapper flags.

## Thresholds and decision states

The decisive capture envelope is the foundation contract: states visible for at least 100 ms appear in a source frame in at least 95% of runs, and states visible for at least 50 ms appear in a source frame in at least 80% of runs. A movement sequence also needs pre-motion, multiple forward states, reversal evidence, correction, and final state. Timing integrity requires separate source/observed/session clocks, deterministic local ordering, explicit known loss, stable anchors, and visible gap boundaries; ordinal arithmetic must not infer unknown Chrome loss.

Browser control requires at least 95% successful action completion on the declared static/moderately dynamic benchmark, with every failure explicit and every state-changing action returning a live observation or a structured observation failure. Storage/retention and performance use the concrete invariants in `docs/EVALUATION.md`: budget and open-segment bounds, pin/eviction/recovery/deletion behavior, bounded memory, and the stated cached/uncached query latency targets under declared hardware and viewport settings. These are independent gates, not one aggregate score.

The product-thesis assessment requires all of the following with the manifest's minimum coverage:

- temporal evidence improves correct transient-defect identification by at least 25 percentage points over final-screenshot inspection;
- the improvement appears across movement, flicker, and layout families, not only one fixture family;
- the temporal bundle performs at least as well as uniform storyboards without using more source-frame tiles; and
- stable-control false positives do not increase by more than 10 percentage points, while every accepted visual claim traces to retained source evidence.

Use these states at both row and aggregate level:

- `pass` — required inputs and sample minimums are complete, all applicable gates pass, and the claim is within the named platform/model/configuration boundary;
- `fail` — required evidence is complete and a measured threshold or correctness invariant is not met;
- `inconclusive` — evidence exists but sample minimums, source retention, required conditions, model metadata, gap-free decisive observations, or cross-platform/model coverage are insufficient to assess the claim;
- `blocked` — a required precondition or operator authorization prevented execution; record the concrete recovery action; and
- `skipped` — only for explicitly optional configurations such as unavailable Linux Chromium, with the skip reason preserved and excluded from any claim of coverage.

No state is promoted to `pass` by treating a skip, timeout, unavailable environment, gap, missing source, scorer ambiguity, or unauthorized paid run as success. A complete run can fail; an incomplete run cannot establish a pass or fail threshold and remains inconclusive.

## Claim boundaries

- **Capture:** claims are bounded by fixture, duration, repetition count, browser product/version/protocol, operating system, architecture, viewport, device scale, image/capture configuration, and observed run revision. They do not guarantee every rendered frame, presentation-time-perfect timestamps, hidden-tab continuity, or durations below the demonstrated envelope.
- **Artifacts:** claims are bounded by exact source frames, gaps, algorithm/adapter versions, parameters, output hashes, and evidence condition. An artifact is a lossy source-derived view; visual measurements are not diagnoses, causes, or ground truth.
- **Agent results:** claims are bounded by provider/model/version, prompts, tools, fixture order/seed, evidence condition, available retrieval actions, and scoring rubric. Initial Codex evidence is Codex-specific. No claim of cross-model generality is made without a separately authorized independent family.
- **Debugging:** patch and verification success measures assistance for the named task. It does not prove automatic defect diagnosis, causal attribution, deterministic replay, framework-state support, all-browser support, or regression-free behavior outside the committed scenarios.
- **Platforms:** the required Linux and macOS rows must be present and decisive before using “cross-platform” language. Linux Chromium is optional and separately labeled. One platform or a wrapper configuration is not evidence for another.
- **Prerequisites:** completed temporal-debugging, transport, and capture-smoke evidence can establish implementation prerequisites and historical context; none is silently broadened into the claims of this epic.

## Qualification and review strategy

The deterministic feature is qualified in CI with locked dependencies, no ignored-output comparison, canonical schema/sample round trips, exact source/artifact hashes, and failure-state tests. The live and platform features are qualified by opt-in runs that validate their own manifests, cleanup, source retention, gaps, and non-claims; a successful run must leave enough local evidence for an independent operator to recompute its score. Manual model features are qualified only after an operator reviews prompt equivalence, authorization, model identity, scenario coverage, raw-answer preservation, and scoring rationale.

Child stories spawned during feature design are implementation checkpoints and advance directly to `done` after green verification; they do not enter review. Each completed feature receives the normal feature-level review and qualification pass. The epic receives an aggregate review only after its feature evidence is complete, checking capability coverage, dependency ordering, claim boundaries, and honest status aggregation. This design transition itself does not invoke paid evaluation or peer review; no child feature is implemented here.

## Decomposition

The epic is split by capability and evidence authority rather than by code layer. The deterministic corpus and manifest contracts come first; deterministic scoring then gives live and manual lanes a stable comparison surface. Live system qualification precedes the platform lane contracts. After that, the platform matrix branch and the Linux reference-host → manual interpretation → debugging branch proceed independently: only the exact Linux reference-host child gates manual model work, while macOS lanes remain cross-platform evidence inputs. This preserves CI parallelism where it is safe without allowing a fake run, one platform, or one model to stand in for another evidence class.

### Child features

- `epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts` — committed defect/control corpus, hidden ground truth, benchmark matrix, prompts, and exact manifest contracts — depends on: `[]`
- `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions` — CI-safe condition packager, deterministic artifact/provenance checks, and structured scorer — depends on: `[epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts]`
- `epic-prove-temporal-advantage-live-capture-and-system-qualification` — opt-in production capture-duration, control, retention, timing, and performance qualification — depends on: `[epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts, epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions]`
- `epic-prove-temporal-advantage-platform-evidence-collection` — required Linux/macOS and best-effort Linux Chromium evidence collection with platform-bounded claims — depends on: `[epic-prove-temporal-advantage-live-capture-and-system-qualification]`
- `epic-prove-temporal-advantage-manual-multimodal-interpretation` — manually authorized Codex-specific interpretation comparison across evidence conditions; requires one declared Linux stable-Chrome reference-host live evidence checkpoint — depends on: `[epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions, epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence]`
- `epic-prove-temporal-advantage-agent-debugging-qualification` — manually authorized reproduce–diagnose–patch–verify benchmark and thesis assessment; consumes the manual contract and does not wait for platform matrix completion — depends on: `[epic-prove-temporal-advantage-manual-multimodal-interpretation]`

### Simplification arcs

- Corpus and manifest contracts — reuse existing fixture/server and evidence-schema conventions; do not introduce framework-state instrumentation or a product evaluation command.
- Deterministic scoring — reuse temporal-vision's authoritative measurements/manifests, existing progressive evidence contracts, and fake storage seams; do not create a second selector, scorer registry, or provenance schema.
- Live/system qualification — reuse production connector, capture status, locks, cleanup, retention, and recovery authorities; do not create a benchmark-only browser or retention implementation.
- Platform evidence — keep transport, capture smoke, and thesis evidence in separate versioned schemas; remove any temporary output publication path that can be mistaken for committed evidence.
- Manual lanes — keep paid calls outside CI and preserve raw local evidence; do not add an abstraction that promises cross-model interchangeability or automatic retries that change samples.

## Decomposition risks

- Linux stable Chrome reference-host availability is a real prerequisite for manual interpretation and must remain an explicit operator blocker; no other platform can substitute for it.
- MacOS/default-DPI/high-DPI availability can leave the platform matrix `inconclusive`, but must not block a valid Linux reference-host agent/model evaluation. The graph must never silently claim macOS from Linux evidence.
- The existing cross-platform smoke has an intentionally absent high-DPI result. A future platform implementation must observe and validate device scale through production metadata; wrapper flags alone are not evidence.
- Manual Codex evaluation can be expensive and cannot be authorized implicitly. Until the operator supplies authorization, a Linux reference interval, and the required sample coverage, the thesis remains `blocked` or `inconclusive`, not failed by default and not passed by default.
- Capture gaps, retention eviction, or missing source frames can make a visual answer unscorable even when an artifact was generated. The manifest and scorer must preserve that distinction rather than reward confident guesses.
- The graph deliberately separates platform comparison from reference-host effectiveness: macOS lanes can run or remain unavailable without delaying manual interpretation, while the exact Linux child preserves the live-evidence guarantee. This is safer than making a whole platform parent a late-bound gate.
