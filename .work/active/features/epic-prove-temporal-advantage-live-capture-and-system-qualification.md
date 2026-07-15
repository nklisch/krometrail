---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification
kind: feature
stage: review
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts, epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Live capture and system qualification

## Outcome

Provide an explicit-opt-in, local-only real-browser qualification harness that exercises the
production browser connector, capture pipeline, recording store, retention/recovery paths,
progressive retrieval, temporal queries, and artifact services as one composed runtime. It
qualifies whether the already-defined temporal-evidence matrix can be captured and retained
honestly; it does not add a product command, a benchmark-specific browser runtime, a model lane,
or a new storage/retention authority.

The harness records a durable, privacy-safe `RunManifest` under the ignored
`target/temporal-evaluation/` boundary. A live qualification is a capture result, not a model
interpretation result: it emits no `EvaluationResultRecord`, answer, transcript, model identity,
or thesis claim. Later interpretation evidence may consume its manifest digest and source
interval, but cannot be manufactured by this feature.

## Design decisions

### One production composition, test-only entry point

The implementation lives behind a test-only entry point in `src/app/live_evaluation.rs`. That
module composes the same ports and concrete authorities used by `build_runtime`:
`ProductionBrowserConnector`, `RecordingStore`, `TemporalVisionArtifactService`,
`ProgressiveEvidenceService`, and `TemporalDebugBundleService`. The harness calls those ports
in-process; it does not invoke MCP, spawn the product binary, add a CLI subcommand, or create a
second adapter with looser semantics.

The existing CDP test helpers are promoted to a narrowly gated `qualification-support` surface in
`crates/krometrail-cdp/src/qualification_support/`. The current real-browser lock, browser
identity gate, managed `ChromeWrapper`, profile cleanup, and loopback static-server behavior are
moved/re-exported from `crates/krometrail-cdp/tests/support/` rather than copied. This keeps one
launcher/lock/cleanup implementation while leaving the helper surface out of the default product
API. The benchmark target remains the dependency-free standalone application under
`tests/fixtures/browser/temporal-benchmark/`, not a browser runtime embedded in Krometrail.

A shared composition helper accepts a test-owned data directory and budget instead of calling
`data_directory()` or opening the operator's persistent store. It returns the same
`RuntimeDependencies` authorities, plus the concrete `RecordingStore` handle needed for
qualification inspection. All scenario stores are temporary directories below the ignored live
output root and are opened through `open_storage_with_budget`; they are not a second storage
implementation or retention policy.

### Explicit opt-in and two evidence layers

The test is `#[ignore]` and requires both the existing real-browser authorization gate and a
feature-specific opt-in (`KROMETRAIL_LIVE_CAPTURE_EVALUATION=1`). Without both, it performs no
fixture-server startup, profile creation, browser discovery, or output write. The ordinary Rust
quality gate therefore remains browser-free. With the opt-in present, a missing required Chrome,
unsupported protocol, fixture failure, viewport mismatch, or failed preflight writes an honest
`blocked` manifest with a recovery action; it never turns absence into a pass.

Code/harness qualification is separate from operator-authorized live evidence collection:

- ordinary unit/contract tests qualify canonical serialization, scripted transport sequencing,
  status precedence, barriers, cleanup, retention/recovery orchestration, and resource/latency
  unavailable handling without launching Chrome;
- an operator-authorized run is the only path that may record observed browser identity, captured
  frames, real source/observed/session timings, real gaps, resource samples, or live artifact
  latency; and
- a green code/harness test is never described as real-Chrome evidence, cross-platform evidence,
  high-DPI evidence, model effectiveness, or a thesis comparison.

The harness accepts a browser product selection from the existing installation registry. Required
Chrome absence is `blocked`; optional Linux Chromium absence is `skipped` with its preserved
reason. An observed run that cannot satisfy a required measurement is `inconclusive`. A complete
run below an applicable threshold or with a violated invariant is `fail`. `pass` requires every
required row and gate to be complete and passing. A no-opt-in invocation is not a result and
writes nothing; a preflight run with opt-in is where blocked/skipped status is materialized.

### Existing manifest is the only live result contract

Do not create `live-qualification.json`, a parallel result schema, or a compatibility alias. Add
the live qualification profile directly to the existing prepublic `RunManifest` contract:

- register `threshold_profile = "live-qualification-v1"` beside the existing profiles;
- add a typed `qualification` block to `RunManifest`, required for that profile and absent for
  unrelated manifests;
- add one canonical `PromptId::CaptureQualification` registry entry whose text explicitly says
  that no model answer is requested, and use `ModelAvailability::NotRequired`; and
- extend the existing manifest status validator and generated schema/sample tests in place.

This is a direct prepublic contract update, not a migration layer. There are no legacy field
names, permissive unknown fields, alternate manifest versions, or model-result shims. The existing
`EvaluationResultRecord` remains the authority for interpretation/debugging answers and is not
written by this feature.

The qualification block has this typed shape (names are normative; field ordering follows the
existing canonical serializer):

```rust
pub struct LiveQualification {
    pub profile: String, // "live-qualification-v1"
    pub gates: Vec<QualificationGateResult>,
    pub capture: CaptureQualificationMeasurements,
    pub control: ControlQualificationMeasurements,
    pub retention: RetentionQualificationMeasurements,
    pub recovery: RecoveryQualificationMeasurements,
    pub resources: ResourceQualificationMeasurements,
    pub latency: LatencyQualificationMeasurements,
    pub cleanup: CleanupQualificationMeasurements,
}

pub struct QualificationGateResult {
    pub gate: QualificationGateId,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}
```

`QualificationGateId::ALL` is the single ordered registry for `capture_envelope`,
`timing_integrity`, `movement_sequence`, `control_reliability`, `retention`, `recovery`,
`resource_usage`, `temporal_query_latency`, `artifact_latency`, and `cleanup`. Every gate appears
exactly once in registry order. The top-level manifest status is the existing precedence over
rows and gates (`blocked`, `skipped`, `inconclusive`, `fail`, `pass`), and the first failing or
incomplete gate supplies the existing explicit `FailureRecord` code/recovery. The validator is
extended so a complete gate failure can make a live run fail without inventing a fake trial row.

Measurements are summaries only; trial-level identity remains in ordinary manifest rows and the
existing source-interval/condition-package contracts. They include exact integer counts and
ordered IDs, no image payloads or private paths:

- `capture` records the requested durations, repetitions, observed viewport/device scale,
  source-frame count, observed-frame count, source-time sample count, gap IDs/counts, and
  per-duration eligibility/coverage rates;
- `control` records the existing operation-registry scenario IDs, attempts, successes, explicit
  failed observations, and exact success rates;
- `retention` records budget, peak usage, pin preservation, eviction, pause/resume, and cleanup
  observations from the concrete store;
- `recovery` records reopen/reconcile outcomes, recovered/removed frame counts, trailing-segment
  repair, and artifact staging recovery from the existing recovery path;
- `resources` records sample count, process RSS/CPU observations, browser-child accounting when
  available, and an explicit unavailable reason when the platform adapter cannot measure them;
- `latency` records source interval identity, viewport/frame dimensions, aggregate cache
  disposition derived from exact artifact identities (including mixed generated/hit output sets),
  temporal-query elapsed values, artifact elapsed values, sample counts, and applicable evaluation
  threshold identities; and
- `cleanup` records only safe booleans/counts for server stop, profile deletion, store flush,
  lock release, output finalization, and zero remaining managed resources.

The manifest input digest excludes measured qualification outcomes just as it excludes ordinary
row outcomes, while retaining the run configuration, fixture hashes, capture configuration,
browser selection, and profile identity. Every live output references the exact source interval
digest and authority-generated artifact/cache identities; no hand-authored provenance is accepted.

### Capture matrix, clocks, gaps, and fixture observation

The harness consumes `temporal-evaluation`'s canonical benchmark definitions and matrix ordering:
all committed case IDs, durations `16|33|50|100|200 ms`, and the registered capture repetition
count. It does not maintain a second duration/case list. Each trial uses the current temporal
benchmark fixture URL with its `case` and `duration_ms` query, the existing structured `Run
interaction` control, and one production capture session. Each trial resolves one interaction-
anchored source interval with a fixed bounded lead/trailing window large enough to include the
fixture's 100 ms lead-in, requested active interval, reversal/correction, and final settle. The
same resolved interval is passed to all later artifact/query measurements for that trial.

Each source frame preserves the existing distinct clocks: CDP source timestamp (when present),
frame observed time, and normalized Krometrail session time. The harness reports source/observed
pairs and all declared `CaptureGapStore` gaps directly. It never infers a gap from frame ordinals,
never fills a missing frame, and never treats the fixture's intended duration as proof that Chrome
presented or Krometrail captured every display frame.

The benchmark intentionally has no labels or hidden framework state. A test-only
`TemporalFixtureObservation` helper decodes retained source-frame bytes and applies narrow,
committed fixture-pixel predicates for only the declared state/motion observables (including
movement path, reversal, teleport, flicker, layout, and stable-state reachability). It is not a
product vision algorithm, does not render or alter evidence, and emits `unknown` on decode,
scale, or geometry mismatch. The predicates are synchronized with the existing benchmark
geometry/definition tests. A frame-classifier result can establish that an actual retained image
contains a declared fixture state; it cannot create a missing frame or make a gap disappear.

The default live profile requires the canonical 800x450 viewport at device scale one and records
what Chrome actually reports. It does not add a high-DPI flag, high-DPI threshold, or platform
claim. The known high-DPI evidence gap remains an explicit non-claim and an operator blocker for
any high-DPI conclusion; the platform-evidence feature owns that lane. A non-one scale or wrong
viewport blocks the canonical capture profile rather than being silently normalized.

### Barriers, control reliability, and cleanup

All scenario transitions use observable production barriers, not arbitrary sleeps:

1. acquire the existing serialized browser lock before any launch;
2. start the loopback fixture server and wait for its bound readiness signal;
3. launch the managed wrapper/profile and wait for the existing connector readiness/target
   attachment barrier;
4. set/verify viewport metrics before the first qualifying trial and require the observed capture
   metadata to match the requested profile;
5. navigate to the exact fixture URL and wait for the current page-ready/target-ready observation;
6. perform a structured operation through the production control port, retaining its interaction
   ID and pre/post live observations;
7. wait for the fixture's observable `running=false`/button-enabled settle condition and for the
   capture sink to acknowledge the required source boundary; and
8. resolve the interval from recorded interaction evidence, then query/store/artifact services
   until their explicit completion handles resolve or their bounded operation deadline returns an
   honest failure.

The live control matrix reuses the existing browser operation registry and real interaction
fixtures (`verified-interactions` and `waits-and-batches`) for navigation, selection, snapshots,
screenshot, click/fill/type/press/select/hover/drag/scroll, dialog, upload, evaluate, wait, and
batch families. It records the production operation outcome plus required post-action
observation; a transport acknowledgement without an observed state is not a successful control
attempt. Unsupported fixture capabilities are incomplete evidence, not a successful no-op.

Cleanup is a finally/Drop path with independent bounded steps: stop capture and flush the writer,
close browser targets, stop the loopback server, release the profile lock, recursively delete the
managed temporary profile, close/reopen or remove the temporary store according to the recovery
scenario, and atomically finalize the manifest only after cleanup evidence is recorded. A cleanup
failure leaves a non-passing manifest with safe resource counts and a recovery action; it never
logs raw profile paths or claims that cleanup succeeded. Cleanup is attempted after timeout,
transport loss, panic/unwind guard paths where possible, and signal-like cancellation exposed by
existing shutdown hooks.

### Retention, recovery, resources, and latency

Retention qualification drives the same `RecordingStore` implementation through a deliberately
small temporary budget. It verifies bounded usage, pinning of a resolved interval, eviction of
unpinned history, explicit capture pause/resume when all candidates are pinned, artifact-linked
retention behavior, and unpin/delete cleanup. Recovery closes the store after a controlled
interruption, reopens through the existing `recover` path, and verifies trailing open-segment
repair, corrupt/staged artifact handling, frame/gap reconciliation, and usage accounting. These
are concrete production paths with temporary data, not a benchmark-specific retention authority.

Resource samples use a platform adapter scoped to the qualification process (the product's
composition process) and identify when browser-child accounting or RSS/CPU data is unavailable.
They report observations rather than inventing a host-performance threshold. The only latency
thresholds used for a decisive gate are the thresholds already stated in `docs/EVALUATION.md`:
for the declared two-second 1080p performance profile, cached temporal bundle retrieval is below
1 s and uncached storyboard/difference-map generation is below 5 s. The harness records dimensions,
range length, warm/cold disposition, and elapsed values so an 800x450 capture-fidelity run cannot
be misreported as a 1080p performance claim. No other host-speed threshold is introduced.

Latency calls go through the existing `TemporalQuery`, `ProgressiveEvidence`,
`TemporalDebugBundles`, and `ArtifactGeneration` ports over the same store and source interval.
The first call establishes the authority-produced cold artifact/cache result; the repeat call
measures the existing cache identity. A cache hit, storyboard, difference map, manifest, output
hash, and resolved range are accepted only from those services. The harness does not decode and
re-render artifacts to make them look available.

## Architectural choice

Three approaches were considered. A new product evaluation command would be easy for operators to
find, but would expand the product surface, create a second runtime lifecycle, and make ordinary
CI/browser availability harder to control. A standalone CDP/store benchmark runner would isolate
long runs, but would duplicate composition, retention, artifact, and provenance authorities and
could silently diverge from production. The chosen approach is a test-only composition-root
harness plus a small shared CDP qualification-support surface: it keeps the expensive operator
path opt-in while exercising the same injected production graph and retaining one manifest
contract. The tradeoff is a larger test-only seam in `src/app`, which is preferable to a second
runtime or an unreviewed CLI.

## Tricky unit first: honest capture qualification output

The highest-risk unit is not launching Chrome; it is deciding when a real run has enough evidence
to say `pass` without converting gaps, unsupported measurements, or fixture timing assumptions into
claims. The `LiveQualification` block and `TemporalFixtureObservation` therefore precede the
scenario breadth. They keep source/observed/session clocks separate, use authority-returned gap
and retention identities, return `unknown` for unclassifiable pixels, and let the existing
manifest validator reject unsupported status transitions. The fallback for an unavailable pixel
predicate, scale, resource adapter, or artifact result is explicit inconclusive/blocked output,
not a weaker heuristic.

## Implementation units

### Unit 1: gated support and shared runtime

**Files**: `crates/krometrail-cdp/src/qualification_support/`,
`crates/krometrail-cdp/tests/support/{chrome.rs,mod.rs,static_fixture.rs}`,
`src/app.rs`, `src/app/live_evaluation.rs`

```rust
pub enum OptInDecision { Disabled, Authorized }
pub enum BrowserPreflight { Ready(BrowserInstallation), Blocked(FailureRecord), Skipped(FailureRecord) }
pub async fn run_preflight(config: LiveQualificationConfig) -> Result<PreflightResult>;
pub async fn finalize_manifest(run: RunManifest, cleanup: CleanupObservation) -> Result<PathBuf>;
```

The unit promotes existing wrapper/lock/server/profile helpers, builds the one production
authority graph over a test-owned store, enforces side-effect-free opt-out, and provides bounded,
idempotent lifecycle cleanup.

### Unit 2: manifest/profile and capture observation

**Files**: `crates/temporal-evaluation/src/{manifest.rs,prompts.rs,matrix.rs,lib.rs}`,
`crates/temporal-evaluation/tests/live_qualification.rs`,
`src/app/live_evaluation/{capture.rs,fixture_observation.rs}`

```rust
pub struct LiveQualification { /* ordered gate results and typed measurements */ }
pub enum QualificationGateId { /* registry-backed, with ALL in canonical order */ }
pub struct TemporalFixtureObservation { /* actual retained-frame classification only */ }
pub fn observe_fixture_frame(frame: &[u8], definition: &CaseDefinition)
    -> Result<FixtureStateObservation>;
```

This unit adds `live-qualification-v1` directly to the existing manifest contract, updates the
canonical no-model prompt identity, consumes the existing matrix, resolves one source interval per
trial, and records real source/observed/session timing, gaps, viewport, and movement state.

### Unit 3: control barriers and reliability

**Files**: `src/app/live_evaluation/{barriers.rs,control.rs}`,
`crates/krometrail-cdp/tests/temporal_benchmark_live.rs`

```rust
pub async fn wait_for_ready_barrier(ctx: &mut LiveTrialContext) -> Result<ReadyObservation>;
pub async fn execute_control_trial(
    ctx: &mut LiveTrialContext, scenario: &ControlScenario,
) -> Result<ControlAttempt>;
```

The unit derives operation identities from the existing registry, requires post-action
observations, and recovers only through a new observable target/capture barrier after timeout or
transport loss.

### Unit 4: store, recovery, resource, and latency measurements

**Files**: `src/app/live_evaluation/{retention.rs,recovery.rs,resource_usage.rs,latency.rs}`

```rust
pub async fn qualify_retention(ctx: &mut QualificationRuntime) -> Result<RetentionObservation>;
pub async fn measure_latency(ctx: &mut QualificationRuntime, interval: &SourceInterval)
    -> Result<LatencyObservation>;
```

The unit uses the existing store/recovery/artifact/query/progressive ports, reports platform
measurement availability, and applies only the two EVALUATION latency thresholds to the declared
1080p performance profile.

### Unit 5: manifest finalization and operator evidence

**Files**: `src/app/live_evaluation/report.rs`,
`crates/temporal-evaluation/tests/live_qualification.rs`,
`docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub fn assemble_manifest(observations: QualificationObservations) -> Result<RunManifest>;
pub fn validate_non_claims(manifest: &RunManifest) -> Result<()>;
```

This unit joins rows, gates, measurements, non-claims, and cleanup evidence, writes one atomic
manifest under the ignored output root, and documents blockers without adding a product command or
model result.

## Simplification

Promote and re-export the existing CDP test helpers instead of copying a wrapper, lock, server, or
profile cleanup implementation. Reuse `RunManifest`, `SourceInterval`, condition packages,
production stores, artifact/cache identities, gap records, and status/failure types instead of
adding live-specific parallel schemas. Keep the live runner test-only and leave the high-DPI lane
with platform evidence. No standalone cleanup/refactor story is needed because the helper
promotion is a cohesive prerequisite of Unit 1 and preserves behavior.

## Testing

- Contract tests protect canonical manifest/schema/prompt/matrix identity, gate completeness,
  status precedence, input-digest exclusion, privacy, and no-model/no-network invariants.
- Scripted composition tests protect one-store authority sharing, barrier ordering, timeout and
  cleanup recovery, retention/recovery outcomes, cache disposition, and unavailable resource
  handling without Chrome.
- Focused fixture-observation tests protect pixel predicate geometry, decode/scale mismatch,
  movement/flicker state ordering, declared gaps, and unknown-state behavior.
- An opt-in-only integration test exercises the real browser path and writes only ignored output;
  it is never part of the ordinary Rust gate and has no fabricated fallback.
- No low-value test is added for every registry row or serializer branch; registry completeness,
  representative boundary mutations, and end-to-end status transitions provide the useful seam
  coverage.

## Risks

- Chrome/Chromium versions, CDP screencast cadence, and platform process metrics can vary; the
  design records observed facts and marks unsupported/missing measurements rather than asserting
  host-independent performance.
- The current fixture has no visible labels, so pixel predicates may be too ambiguous after a
  browser rendering change; definition/geometry drift must fail closed and the run must become
  inconclusive rather than silently passing.
- The existing high-DPI evidence gap remains unresolved. A non-one scale blocks the default
  profile, and no high-DPI conclusion is emitted here.
- The full matrix is expensive and can exhaust temporary disk or operator time. Bounded per-trial
  deadlines, retention budgets, and cleanup evidence limit damage, while an interrupted run stays
  incomplete rather than being summarized as a pass.
- A future implementor might be tempted to make a sidecar result or CLI for convenience; the
  manifest/profile and no-product-command constraints are deliberate and must remain enforced by
  contract tests.

## Implementation order

1. Unit 1 — opt-in, support helpers, shared authority graph, lifecycle and manifest skeleton.
2. Unit 2 — canonical matrix, live profile, interval/timing/gap/movement observations.
3. Unit 3 — control registry, post-action reliability, deterministic barriers and recovery.
4. Unit 4 — retention/recovery, process resources, query/artifact cache latency.
5. Unit 5 — status aggregation, non-claims, atomic output, cleanup verification and operator
   boundary documentation.

## Child checkpoints

The implementation is intentionally sequential. Each child completes one authority boundary and
leaves a manifest/status contract that the next child can consume; no child starts Chrome during
ordinary qualification.

1. `epic-prove-temporal-advantage-live-capture-and-system-qualification-opt-in-harness-and-live-run-contract`
   establishes the gated support helpers, shared runtime composition, output boundary, live
   manifest/profile/prompt contract, status precedence, preflight, deterministic lifecycle, and
   no-Chrome qualification tests.
2. `epic-prove-temporal-advantage-live-capture-and-system-qualification-duration-capture-timing-and-movement`
   consumes the canonical matrix and implements one-session duration sweeps, source/observed/
   session timing, declared gap propagation, fixture pixel observation, and movement coverage.
3. `epic-prove-temporal-advantage-live-capture-and-system-qualification-control-reliability-and-session-barriers`
   adds the operation-registry control matrix, observable post-action reliability accounting,
   deterministic readiness/settle barriers, and timeout/transport-loss recovery.
4. `epic-prove-temporal-advantage-live-capture-and-system-qualification-retention-recovery-and-performance`
   exercises concrete retention/recovery, resource sampling, source retrieval, temporal queries,
   artifact generation/cache, and the EVALUATION-scoped latency profile.
5. `epic-prove-temporal-advantage-live-capture-and-system-qualification-manifest-status-and-operator-evidence`
   assembles canonical manifests, non-claims, output finalization, cleanup evidence, qualification
   tests, operator instructions/blockers, and the final separation between system qualification and
   authorized live evidence.

## Acceptance evidence

- [ ] With the opt-in absent, ordinary workspace tests do not start Chrome, bind a fixture server,
      create a managed profile, touch the operator data directory, or write live output.
- [ ] Scripted/no-browser tests prove canonical live manifest serialization, registry completeness,
      status precedence, privacy/path rejection, failure recovery, deterministic barrier ordering,
      cleanup idempotence, and one shared production authority graph.
- [ ] An opted-in run with no required browser writes `blocked` with an explicit recovery action;
      optional Linux Chromium absence is `skipped`; incomplete data is `inconclusive`; complete
      below-threshold data is `fail`; no fabricated pass is possible.
- [ ] A successful run covers every canonical case, duration, and required repetition, preserves
      source/observed/session timing distinctions and declared gap IDs, and records movement and
      control outcomes from actual production observations.
- [ ] Retention/recovery/resource/query/artifact measurements identify their scope, source interval,
      cache disposition, dimensions, and unavailable reasons without adding host thresholds outside
      `docs/EVALUATION.md`.
- [ ] The emitted manifest contains no image bytes, base64, model answer, transcript, raw page text,
      absolute/private path, remote URL, credential, or unregistered provenance; high-DPI and model
      effectiveness remain explicit non-claims.
- [ ] Every temporary browser profile, lock, server, store, and staging artifact is cleaned or an
      honest cleanup failure is recorded before output finalization.
- [ ] No product CLI command, remote/paid/model call, benchmark browser runtime, second
      storage/retention authority, generated documentation, or `.work/bin/work-view` change is
      introduced.

## Operator blockers and non-claims

This design does not launch Chrome. Before an operator-authorized collection can be meaningful,
the operator must provide a locally installed supported Chrome/Chromium, permission to create and
delete a temporary managed profile and loopback listener, a clean working tree for the committed
fixture/definition hashes, and enough temporary disk for the configured capture and retention
runs. The operator must explicitly acknowledge that default-DPI evidence is the only capture
profile here; the existing high-DPI evidence gap is not repaired or converted into a claim.

A live `pass` means only that the declared local configuration completed the declared production
capture/storage/latency qualification. It does not prove cross-platform parity, high-DPI support,
model accuracy, interpretation quality, temporal-advantage uplift, ordinary host speed, or
production-scale stability. Those claims require the platform-evidence, deterministic-scoring,
and later authorized model lanes.

## Implementation notes

- Execution capability: feature-owning Luna worker, direct design write with sequential child
  checkpoints; no sub-agent or cross-model review was needed for this bounded contract design.
- Review weight: standard integrated feature review after all child checkpoints; children are
  verification checkpoints and do not each require a separate review lane.
- Design-only scope: no browser was launched, no live output was generated, and no product runtime
  or CLI behavior was changed in this stride.
- Live latency correction: aggregate cache state is now derived from every authority-returned
  `ArtifactCacheDisposition`; `Mixed` is distinct from all-generated `Cold`, all-hit `Warm`, and
  `Unavailable`. The direct artifact pair remains the uncached `<5 s` target and the repeated
  all-hit bundle remains the warm `<1 s` target. The first bundle is intentionally recorded as
  mixed when the direct request has warmed its shared difference-map key.
- Adjacent decisions: the high-DPI gap remains owned by platform evidence; no backlog item was
  created because it is an explicit upstream contract/blocker rather than an incidental bug.

## Integrated implementation evidence

- The final checkpoint is implemented in one report authority: it assembles registry-ordered
  qualification gates, derives honest status precedence, carries typed evidence mode and fixed
  live non-claims, validates complete-pass invariants, and atomically publishes only beneath the
  ignored `target/temporal-evaluation/live/<browser>/<run>/` boundary with a safe finalization-error
  report. The prior duplicate finalizer was removed from the composition module.
- The child checkpoint is legitimately `done`; all preceding capture, control, retention/recovery,
  and latency authorities remain its inputs. The parent feature stays at `stage: review` for the
  integrated feature review.
- Final focused verification: `cargo fmt --all -- --check`; root report tests (5 passed) with
  `--features qualification-support`; `temporal-evaluation` live qualification tests (9 passed);
  and manifest contract tests (5 passed). Prior Rust 1.85 locked default, qualification-support,
  and qualification-support CDP check/test/clippy gates remain applicable; no code correction was
  made after those full gates.
- Actual operator-authorized live evidence was **not run and remains uncollected**: no live browser
  identity, source frames, gaps, resource samples, latency measurements, or live pass is claimed.
  No Chrome, model, paid/remote service, performance/report follow-up, or backlog work was started;
  `.work/bin/work-view` was not restored, staged, or committed.
