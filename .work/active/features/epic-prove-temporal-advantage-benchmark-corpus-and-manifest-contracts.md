---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
kind: feature
stage: review
tags: [testing, visual, browser]
parent: epic-prove-temporal-advantage
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Benchmark Corpus and Manifest Contracts

## Brief

Deliver the committed benchmark vocabulary for the temporal-advantage program: deterministic movement-reversal, flicker, transient-layout, DOM-opaque-motion, and stable-control target applications, each with hidden machine-readable ground truth and controlled duration/variant metadata. The fixtures remain standalone browser targets, not a second Krometrail runtime and not framework-state test subjects. Existing browser fixtures, shared local-server patterns, and the current cross-platform smoke fixture should be reused where they fit instead of creating duplicate launch or target abstractions.

Define the benchmark matrix, evidence-condition identifiers, structured interpretation/debugging task prompts, deterministic ordering and seed rules, and the versioned schemas consumed by every later feature. This feature owns the distinction between a known fixture state and a Krometrail observation; ground truth must never be computed from Krometrail measurements. It does not claim that a real Chrome stream captures any duration or that a model understands an artifact.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: contract and corpus foundation — every later harness and evidence lane consumes these identifiers and definitions

## Execution boundary

- CI-safe by default: fixture definitions, ground-truth timelines, prompts, schemas, and matrix configuration are committed and runnable without Chrome, network access, paid agents, or a second model family.
- Live Chrome, cross-platform collection, and model execution are separate opt-in consumers. Missing environments remain explicit unavailable evidence; this feature does not provide fallback passes.

## Simplification opportunity

- Reuse the existing standalone fixture conventions, `tests/fixtures/browser/README.md` boundary, shared Chrome test helpers, and versioned evidence-schema style. Do not add framework-state instrumentation, a product CLI command, a second fixture server framework, or compatibility aliases for unpublished benchmark contracts.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, Local-First Operation, and Success
- `docs/SPEC.md` — Supported Environment, Continuous Visual Capture, Temporal Ranges, and Exclusions
- `docs/ARCHITECTURE.md` — Temporal Visual Crate, Capture Tasks, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Evidence Classes, Capture Gaps, Provenance, and Non-Diagnostic Posture
- `docs/EVALUATION.md` — Benchmark Corpus, Ground Truth, and Visual Interpretation Tasks
- `tests/fixtures/browser/README.md` — target-application boundary

## Design decisions

- **Contract home**: add a browser-agnostic `temporal-evaluation` workspace crate and keep the target application under `tests/fixtures/browser/temporal-benchmark/` — the manifest is shared by later browser, artifact, scoring, and manual lanes without importing evaluation concerns into `krometrail-core`, `krometrail-cdp`, or the product binary.
- **Committed definition authority**: `docs/evidence/temporal-evaluation/v1/benchmark-definition.json` is the one current benchmark-definition input; Rust types generate its JSON Schema and validate its registries. There are no compatibility aliases, migrations, legacy shapes, or hypothetical consumer shims.
- **Fixture strategy**: use one dependency-free static target with case/duration query selection and stable visible interaction anchors. Reuse the existing fixture boundary and browser test support (`ChromeWrapper`, real-browser lock, profile cleanup, and static serving pattern) rather than adding a benchmark launcher, framework instrumentation, or product command.
- **Ground-truth authority**: expected phases, intervals, geometry, final states, intentional-vs-defective labels, and stable-control expectations are authored in the committed benchmark definition. They are never recomputed from Krometrail frames, visual measurements, artifacts, or model answers. The evaluator withholds the definition from agent-facing interpretation inputs.
- **Timing semantics**: durations are exact requested `performance.now()` intervals in the fixture contract, not claims about presentation or capture. The 16 ms row is `frame_approx`, not an exact display-frame guarantee. Source, observed, and normalized session times remain separate in run rows.
- **Ordering**: capture rows use canonical family/case/duration/repetition ordering. Randomized interpretation rows use deterministic Fisher-Yates with seed `0x4b524f4d45545241` and record the realized order. No OS, process, filesystem, or hash-map ordering is observable.
- **Status semantics**: `pass`, `fail`, `inconclusive`, `blocked`, and optional `skipped` remain distinct. Missing Chrome, unsupported platforms, unavailable source, gaps, missing model authorization, and missing minimum samples never become a passing result. Only optional Linux Chromium may use `skipped`.
- **Canonical representation**: schema objects reject unknown fields; canonical JSON uses recursively ordered keys, semantic arrays, finite normalized numbers, bounded UTF-8 strings, and lowercase SHA-256 hashes. Exact prompt text is recorded for model runs, while untrusted raw answers are retained as ignored sidecars referenced by digest so page bodies and sensitive browser data do not enter manifests.
- **Output boundary**: definitions, schemas, prompts, scoring vocabulary, fixture source, and canonical samples are committed. Per-run manifests, frames, artifacts, model answers/transcripts, patch workspaces, logs, and aggregate results stay under ignored `target/temporal-evaluation/`. The existing `target/` rule is sufficient; no generated documentation or product CLI surface is changed.
- **UI applicability**: no UI mockups apply. These are controlled target applications and evidence contracts, not Krometrail product screens or user journeys.
- **Documentation timing**: this design does not contradict a foundation assertion and does not preflight-edit the five foundation documents. Implementation will use the code-first rolling-foundation rule if an assertion becomes false; this feature's evidence README is a versioned contract document, not generated VitePress content.

## Architectural choice

Three options were considered:

1. **Test-only types in `krometrail-cdp/tests/support`** would be quick and could reuse the
   current smoke harness, but later temporal-vision, store, platform, and manual lanes would
   either depend on a test module or copy the manifest. It also makes the external schema less
   clearly browser-agnostic.
2. **Product-domain types in `krometrail-core`** would make the types broadly visible, but
   benchmark vocabulary, model prompts, and evaluation statuses are not runtime browser-domain
   concepts. It would couple product builds to test/evaluation data and invite a benchmark CLI
   into the current intentionally small command surface.
3. **A small `temporal-evaluation` contract crate plus committed definitions** keeps evaluation
   inputs reusable without putting them in the runtime, gives later features one typed boundary,
   and lets JSON Schema be generated from the same types that validate manifests. This is the
   chosen option. It is deliberately not a second browser runtime and has no Chrome, MCP, model,
   filesystem, or network implementation dependency.

## Tricky unit first: honest manifest state and canonicalization

The highest-risk unit is the run manifest because incomplete evidence can look structurally valid
while making an invalid product claim. Its validation must model availability and claim status as
separate facts: an observed browser can still produce an inconclusive row due to gaps; a blocked
model authorization cannot be represented as an empty model object; an evicted source frame
invalidates the claim that depends on it; and a complete below-threshold row is a failure. The
canonicalizer must hash exact inputs without accidentally retaining absolute paths, endpoints,
page text, credentials, or raw adapter errors. This unit is designed before the fixture consumer
so every later lane has one honest failure vocabulary.

## Implementation Units

### Unit 1: deterministic fixture and hidden corpus definition

**Files**:

- `tests/fixtures/browser/temporal-benchmark/index.html`
- `tests/fixtures/browser/temporal-benchmark/benchmark.js`
- `tests/fixtures/browser/temporal-benchmark/benchmark.css`
- `tests/fixtures/browser/temporal-benchmark/README.md`
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.json`

The static target is fixed at an 800x450 CSS-pixel qualification viewport. It accepts only a
canonical case ID and duration from its relative route, resets before each click, uses no network,
randomness, wall-clock time, framework runtime, external asset, or hidden instrumentation, and
runs visual updates from `performance.now()` and `requestAnimationFrame`. The fixture contract
uses half-open phase intervals and a 100 ms lead-in/settle for defect cases. Stable controls run
through the same duration matrix but are intentionally correct.

The committed cases are:

| Case ID | Behavior identity |
| --- | --- |
| `movement-reversal/basic` | x=48 baseline; forward toward x=288; x=160→120 reversal for `D`; correction to x=288; stable final x=288. |
| `flicker/visibility` | status card absent for `D`, then identical final card. |
| `flicker/color` | incorrect red status style for `D`, then neutral final style. |
| `flicker/text` | `Ready` → `Stale data` for `D` → `Ready`. |
| `layout/width` | content width 640 → 480 for `D` → 640. |
| `layout/content-shift` | 48 px notice insertion shifts content for `D`, then removal restores geometry. |
| `layout/scroll-position` | scroll position 0 → 160 for `D` → 0. |
| `dom-opaque/path-reversal` | canvas marker moves toward x=320, reverses to x=240 for `D`, corrects to x=320. |
| `dom-opaque/teleport` | canvas marker is at x=520 instead of x=320 for `D`, then returns. |
| `dom-opaque/sprite` | canvas sprite has the incorrect committed color/shape for `D`, then returns. |
| `stable/smooth-panel` | panel moves monotonically x=48→288 over `D`; no defect. |
| `stable/loading-indicator` | expected loading animation/progress for `D`, then ready; intentional. |
| `stable/caret` | intentional 500 ms caret blink; `D` is observation-window metadata only. |

The target exposes stable interaction anchors but no ground-truth endpoint or defect label. The
benchmark definition records case family, intent, variant, anchor, exact timing rule, phase/state
IDs, defect interval, affected viewport geometry, expected path/transition, final stable state,
and ordered fixture-file SHA-256 identities. The evaluator does not provide this definition to
agent-facing visual interpretation.

**Acceptance criteria**:

- [ ] All case IDs and the duration sweep `[16, 33, 50, 100, 200]` are exhaustive, ordered, and
      reject undeclared query values.
- [ ] Fixture behavior is local and repeatable without Chrome or network; it does not import
      Krometrail or expose framework-state instrumentation.
- [ ] Ground truth is loaded from committed definitions and never derived from captured frames,
      artifacts, or model output.
- [ ] Fixture hashes and phase/final-state assertions are validated before any live consumer runs.

### Unit 2: benchmark matrix, conditions, prompts, and scoring vocabulary

**Files**:

- `crates/temporal-evaluation/src/corpus.rs`
- `crates/temporal-evaluation/src/conditions.rs`
- `crates/temporal-evaluation/src/prompts.rs`
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.json`

One registry defines families, case IDs, condition IDs, prompt IDs, scoring dimension IDs, status
values, and allowed answer vocabulary. The matrix has 30 capture repetitions per case/duration/
configuration and at least 10 randomized interpretation trials per required family/condition.
Capture order is family → case ID → duration → repetition. Interpretation order is the same
trial set passed through seeded Fisher-Yates with seed `0x4b524f4d45545241`; the realized trial
IDs are later written to the manifest.

The only evidence conditions are:

- `A-final-screenshot`: final screenshot and ordinary current observation;
- `B-uniform-storyboard`: exactly eight uniformly positioned source-frame slots;
- `C-change-aware-storyboard`: at most eight change-aware storyboard tiles using the committed
  algorithm descriptor;
- `D-temporal-bundle`: before/during/after, change-aware storyboard, difference map, capture
  summary, and references, with no more than eight source-frame tiles;
- `E-progressive-source`: D plus at most two four-frame retrievals and one declared region
  filmstrip, with every returned or unavailable source identity recorded.

All conditions use the same captured interval. Condition metadata declares its tile budget,
retrieval budget, algorithm/version, parameters, prompt ID, and scoring dimensions. The exact
structured interpretation response is bounded to `temporary_state`, `state_order`,
`affected_region`, `motion_behavior`, `judgment`, `uncertainty_reasons`, and `evidence_refs`.
Allowed scoring dimensions are `transient_defect_identification`, `state_order`,
`affected_region`, `motion_behavior`, `gap_uncertainty`, and `stable_control_false_positive`.
The prompt explicitly preserves Krometrail's non-diagnostic posture and requires uncertainty when
capture gaps or missing source prevent a claim; it does not name the case family, defect mechanism,
or expected answer.

**Acceptance criteria**:

- [ ] Conditions, prompts, scoring vocabulary, seed, order rules, tile limits, and sample minima
      are defined once and are consumable by later features.
- [ ] Repeated matrix generation is byte- and order-identical across supported hosts.
- [ ] Prompt/answer validation rejects undeclared values, unknown fields, overlong strings, and
      certainty unsupported by a gap or missing source.
- [ ] Partial, unsupported, or unauthorized rows have explicit non-passing status semantics.

### Unit 3: versioned run manifest, generated schemas, canonical serialization, and privacy

**Files**:

- `Cargo.toml` (workspace member only)
- `crates/temporal-evaluation/Cargo.toml`
- `crates/temporal-evaluation/src/lib.rs`
- `crates/temporal-evaluation/src/manifest.rs`
- `crates/temporal-evaluation/src/canonical.rs`
- `crates/temporal-evaluation/src/privacy.rs`
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json`
- `docs/evidence/temporal-evaluation/v1/run-manifest.schema.json`

The public contract is:

```rust
pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const BENCHMARK_ID: &str = "temporal-advantage-corpus-v1";

pub fn canonical_json<T: serde::Serialize>(value: &T)
    -> Result<Vec<u8>, ContractError>;
pub fn sha256_prefixed(bytes: &[u8]) -> String;
pub fn benchmark_definition_schema() -> schemars::Schema;
pub fn run_manifest_schema() -> schemars::Schema;

impl BenchmarkDefinition {
    pub fn canonical() -> Self;
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn definition_digest(&self) -> Result<String, ContractError>;
}
impl RunManifest {
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn sanitize(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn input_digest(&self) -> Result<String, ContractError>;
}
```

`RunManifest` has one current v1 shape with required fields for:

- manifest kind/version and benchmark-definition Git revision/digest;
- harness, scorer, fixture root-relative path, ordered fixture file hashes, and definition hash;
- seed, order policy, realized trial order, duration list, repetition count, condition,
  threshold profile, viewport, device scale, image configuration, and retention budget;
- sanitized platform, architecture, and OS release class;
- browser product/version/protocol/revision/capability identity and explicit availability state;
- Krometrail revision, Cargo.lock digest, Rust toolchain, capture configuration, and adapter
  algorithm versions;
- model provider, model ID, model version or dated alias, invocation date, authorization reference,
  tools, input limits, and explicit unavailable/blocked/not-required state;
- exact system/task prompts, prompt ID/version/hash, artifact condition/algorithm/parameters,
  source interval, output IDs/hashes/availability, source-frame IDs, and gap IDs;
- rubric version, score dimension IDs, aggregate method, rationale, accepted-claim evidence IDs,
  raw-answer digest/ignored sidecar reference, and row/aggregate status/non-claims/failure reason.

Rows retain source, observed, and session time separately; capture ordinals order locally but do
not imply unknown Chrome frame continuity. Every accepted visual claim names retained source
identities or remains unavailable. A gap, eviction, corruption, absent model authorization,
unsupported required browser/platform, or insufficient sample minimum prevents `pass`.

Schemas are generated from the Rust `serde`/`schemars` types with unknown fields denied. Canonical
bytes use recursively lexicographic object keys, semantic arrays in declared order, finite
numbers only (`-0` becomes `0`), bounded UTF-8 strings, integer durations/counts, and lowercase
`sha256:<64 hex>` hashes. Privacy validation permits only repository-relative POSIX definition
paths and opaque ignored-output handles; it rejects absolute paths, `..`, backslashes,
URLs/endpoints/ports, usernames, home/profile/temp roots, credentials, page bodies, raw browser
payloads, and raw adapter errors. Raw model prose is an ignored sidecar; canonical metadata holds
its digest, opaque reference, and bounded structured answer.

**Acceptance criteria**:

- [x] The new crate has no dependency on Krometrail runtime crates, Chrome, MCP, a network, or a
      model client; later lanes consume it as their single manifest contract.
- [x] Generated schemas match committed schema files exactly and the canonical sample round-trips
      byte-for-byte.
- [x] Unknown fields, duplicate/unsorted IDs, non-finite numbers, contradictory counts/digests,
      missing observed identities, and passing incomplete rows are rejected.
- [x] Privacy tests reject private machine details and page-sensitive content while accepting
      permitted relative fixture paths and opaque output references.

### Unit 4: CI-safe boundary, committed samples, and evidence README

**Files**:

- `crates/temporal-evaluation/tests/contracts.rs`
- `crates/krometrail-cdp/tests/temporal_benchmark_contract.rs` (only thin integration checks that
  belong to the existing browser test package)
- `docs/evidence/temporal-evaluation/v1/README.md`
- `docs/evidence/temporal-evaluation/v1/sample-manifest.json`

The deterministic test surface validates the definition, generated schemas, canonical sample,
fixture hashes, registries, matrix order, prompt/answer vocabulary, input digest, privacy rules,
and status aggregation. It runs with ordinary locked Rust tests and does not start Chrome, make a
network request, invoke a paid model, or mutate generated VitePress documentation. It reuses the
existing fixture/browser test boundary and support identities rather than adding a launcher or a
second server abstraction.

Git contains only source fixtures, definitions, prompts, schemas, scoring rules, contract code,
README/provenance instructions, and canonical samples. All per-run manifests, source frames,
artifacts, model answers/transcripts, patch workspaces, logs, and aggregate results are written
only below ignored `target/temporal-evaluation/`. The existing `target/` ignore rule is checked;
no new broad ignore or generated `docs/public/llms-full.txt` edit is needed. The README states
that CI proves contract/canonicalization/privacy determinism only, while live Chrome, platform,
artifact, and manual model results are separate opt-in consumers.

**Acceptance criteria**:

- [x] Locked CI tests are green without Chrome, network access, paid agents, or a second model
      family and cannot emit a false live/model pass.
- [x] A clean-checkout test proves committed schema/sample bytes equal generated canonical bytes.
- [x] The ignored-output boundary and privacy checks pass; no standalone refactor item under
      review is touched.
- [x] The README preserves explicit blocked/inconclusive/optional-skipped semantics and names
      the recovery action for unavailable required evidence.

## Integrated implementation evidence

- Corpus checkpoint: committed dependency-free temporal benchmark fixture, hidden ground truth,
  fixture hashes, and canonical definition/schema identities are validated by the contract tests.
- Vocabulary checkpoint: exact condition, prompt, scoring, status, matrix seed, deterministic
  ordering, answer bounds, and metadata non-leakage are validated without a browser or model.
- Manifest checkpoint: strict v1 `RunManifest` availability, retention, gap, status, privacy,
  canonicalization, and input-digest contracts are generated and tested from Rust types.
- CI-boundary checkpoint: `README.md`, `contracts.rs`, ignored `target/temporal-evaluation/`
  output verification, and clean generation/cmp checks complete the feature boundary.
- Final gates: Rust 1.85 locked formatting, workspace check, workspace tests (668 passed, 1
  ignored), workspace clippy with `-D warnings`, and all four generation comparisons passed.
- Scope remained CI-safe: no Chrome launch, network request, model credential, paid agent, product
  CLI, generated documentation edit, or parent-feature review was performed.

## Implementation Order

1. `...-corpus` — static target and exact hidden ground truth.
2. `...-vocabulary-and-prompts` — matrix, conditions, prompts, seeded ordering, and scoring IDs.
3. `...-run-manifest-and-schema` — new contract crate, generated schemas, canonicalization,
   provenance fields, validation, privacy, and status aggregation.
4. `...-ci-boundary-and-samples` — locked deterministic checks, committed samples, README, and
   ignored run-output boundary.

The child story files carry the same order as `depends_on` chains. They are implementation
checkpoints, not separate worker assignments; one feature owner should implement and verify the
cohesive bundle.

## Simplification

- Keep the product CLI unchanged; no evaluation command is introduced.
- Keep fixture targets dependency-free and static; remove any temptation to add React/Vue state
  instrumentation or a framework-specific runner.
- Reuse existing Chrome/profile/lock/static-fixture support. Add only the shared contract crate
  and a minimal fixture-target asset set needed by this corpus.
- Generate schemas from contract types and keep one registry for variants, conditions, statuses,
  prompts, and scoring dimensions; do not maintain hand-copied consumer enums.
- Do not add legacy aliases, migration readers, fallback providers, compatibility shims, or a
  second manifest/provenance schema for unpublished contracts.
- Do not alter the standalone `[refactor]` stories under review.

## Testing

- **Contract/interface tests**: definition and schema round trips, exact canonical sample bytes,
  registry completeness, fixture digest order, condition budgets, prompt vocabulary, and seeded
  trial order protect every later consumer's stable boundary.
- **Invariant tests**: status/availability combinations, gap and retention references, model
  authorization, threshold minimums, and no-pass-on-missing-input protect honest claims.
- **Privacy tests**: path/endpoint/credential/page-content rejection prevents local identity or
  sensitive browser data from entering committed manifests.
- **Fixture tests**: static no-network/no-random/no-wall-clock checks plus exact phase/final-state
  definition checks protect deterministic target behavior without live Chrome.
- **No live/model tests here**: Chrome capture, artifact rendering, platform evidence, and
  paid/manual interpretation belong to downstream features and remain explicit unavailable when
  not authorized. No low-value line-by-line tests or duplicate schema copies are planned.

## Risks

- **Browser timing is not presentation timing**: `performance.now()` phase identities can be
  delayed or skipped by Chrome; the manifest therefore records requested timing separately from
  source/observed timing, and no fixture duration is treated as captured until a later live lane
  observes it.
- **Ground truth leakage**: visible labels, DOM snapshots, or route metadata could trivialize
  interpretation. The target keeps only stable operation anchors and normal content; ground truth
  is a separate evaluator input and prompt templates omit case/defect labels.
- **Schema drift by hand editing**: committed JSON schemas can diverge from Rust validation.
  Generated-schema byte checks and a canonical sample round trip make drift fail in deterministic
  CI.
- **Privacy versus reproducibility**: raw model answers and browser artifacts may be necessary to
  audit a run but can contain page content. The manifest stores bounded structured answers,
  digests, and opaque ignored sidecar references; the sidecars remain local and are not committed.
- **Unsupported environments**: no local Chrome, macOS/high-DPI, optional Chromium, or model
  authorization can block later evidence. This feature records the state vocabulary and recovery
  fields; it does not manufacture substitute evidence or weaken thresholds.

## Foundation and adjacent boundaries

The design is grounded in `docs/VISION.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`,
`docs/VISUAL-EVIDENCE.md`, and especially `docs/EVALUATION.md`. It preserves the existing
standalone target-application boundary in `tests/fixtures/browser/README.md`, the cross-platform
smoke's generated-schema/canonical-sample/redaction conventions, temporal-vision's existing
artifact provenance and gap semantics, Cargo workspace boundaries, and the documented Bun-only
VitePress tooling boundary. No foundation assertion is changed by this design; downstream
implementation must update an assertion in place if its behavior makes one stale.
