---
id: epic-prove-temporal-advantage-platform-evidence-collection
kind: feature
stage: implementing
tags: [testing, browser, infra]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Platform Evidence Collection

## Brief

Collect and qualify the live evaluation matrix on the supported environments named by the
foundation: Linux with stable Chrome and macOS with stable Chrome, including macOS default-DPI and
high-DPI configurations. Linux Chromium remains a separately labeled best-effort configuration.
Each lane consumes the existing production live-qualification harness and its canonical
`RunManifest`; the lane and matrix contracts add no browser, storage, artifact, or model
authority.

This feature has two deliberately separate outcomes. The Linux stable-Chrome reference-host lane
is a required evidence checkpoint for later manual multimodal interpretation and therefore can
unblock agent/model evaluation by itself. The platform matrix is a cross-platform comparison;
missing macOS default-DPI or high-DPI evidence leaves that comparison `inconclusive`, but must not
block the reference-host evaluation or be silently represented as a macOS pass. Optional Linux
Chromium may be `skipped` when unavailable and is never used as Chrome or cross-platform evidence.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: platform evidence branch — compares independently identified live lanes without
  becoming a prerequisite for reference-host agent effectiveness
- Depends on: `epic-prove-temporal-advantage-live-capture-and-system-qualification`

## Execution boundary

- Operator-authorized local/hosted collection only. The design does not launch Chrome, authorize a
  paid model, collect evidence, or assume that an operator can provide macOS.
- Every lane uses the existing test-only `run_live_qualification` composition and writes only
  ignored per-run output below `target/temporal-evaluation/live/`. The lane manifest remains the
  source of exact browser/protocol, platform/architecture, Rust/toolchain/Git, fixture, capture,
  seed, threshold, viewport/scale, gap, retention, and non-claim facts.
- The committed platform matrix contains only typed lane identities, manifest/input digests,
  statuses, failure/recovery records, and claim assessments. It does not copy frames, artifacts,
  model answers, transcripts, private paths, or a second run-manifest schema.
- No platform result is inferred from wrapper flags. The high-DPI lane is decisive only when the
  production capture metadata observes the declared high-DPI scale band; a forced flag with an
  observed scale of `1.0` remains unavailable/inconclusive and emits no passing evidence file.

## Design decisions

- **Reference-host dependency is exact, not transitive through the matrix**: the manual
  interpretation feature depends on
  `epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence`,
  which must leave one declared, operator-authorized Linux stable-Chrome live manifest. It does not
  depend on this platform feature or on either macOS lane.
- **One lane registry**: `PlatformLaneId::ALL` is the source of truth for required/optional status,
  browser product, operating system, DPI expectation, output naming, and claim scope. Runner
  configuration, validation, aggregation, display, and tests derive from it.
- **Existing live manifests remain authoritative**: a platform lane validates an existing
  `RunManifest` against its registry definition and stores its digest in the matrix. The matrix
  is an index and claim assessment, not a second copy of live measurements.
- **Parallel lanes, serialized browser execution**: the lane stories and their output contracts are
  independent after the shared lane contract. The existing real-browser lock serializes actual
  Chrome launches where necessary; graph parallelism must not be implemented as concurrent use of
  one managed profile or one store.
- **Mac absence is non-blocking to reference-host work**: missing macOS installation, failed
  default-DPI observation, or failed high-DPI observation produces `inconclusive` platform
  coverage. It cannot downgrade a valid Linux reference-host status, prevent manual interpretation,
  or be changed into a passing placeholder.
- **No cross-platform language without all required rows**: a Linux pass plus an absent macOS row
  can support only the declared Linux configuration. “Cross-platform” requires decisive Linux
  stable Chrome, macOS default-DPI, and macOS high-DPI rows; optional Linux Chromium is never part
  of that requirement.

## Architectural choice

Three approaches were considered:

1. **Only reuse individual `RunManifest` files and aggregate in prose** would avoid code, but it
   would leave lane identity, missing-row semantics, and cross-platform claim rules implicit and
   make an absent macOS row easy to overstate. Rejected.
2. **Create a platform-specific copy of the live manifest** would make reports convenient, but it
   would duplicate browser, capture, gap, artifact, and status authorities and invite drift from
   `RunManifest`. Rejected.
3. **Use a registry-backed lane runner plus a digest-only matrix assessment (chosen)** keeps
   `RunManifest` authoritative, makes required/optional lanes explicit, and lets a reference-host
   result mature independently of the incomplete macOS matrix. The tradeoff is one small
   evaluation contract and a validation adapter that must load the referenced manifests before
   publishing an aggregate.

## Tricky unit first: honest matrix status and claim support

The highest-risk unit is status aggregation, not wrapper construction. A matrix can look complete
when it has treated a missing macOS row as a pass, accepted a forced high-DPI flag without an
observed scale, or allowed optional Chromium to stand in for Chrome. The aggregator therefore
validates every referenced manifest against the lane registry before deriving three separate
outcomes:

```text
Linux stable Chrome reference manifest ─┐
macOS Chrome default-DPI manifest      ├─▶ lane validation ─▶ reference-host / cross-platform / optional assessments
macOS Chrome high-DPI manifest         │
Linux Chromium optional manifest       ┘
```

The reference-host assessment depends only on the Linux stable-Chrome row. The cross-platform
assessment depends on all three required rows. A missing or non-decisive macOS row makes the latter
`inconclusive`; it does not make the former unavailable. The optional Chromium assessment is
`Skipped` only with its explicit optional-unavailability reason. Complete observed rows can still
be `Fail` when their live qualification thresholds fail; incomplete rows are never promoted to a
threshold result.

## Implementation units

### Unit 1: lane registry, manifest profile, and shared runner contract

**Files**:

- `crates/temporal-evaluation/src/platform.rs` (new)
- `crates/temporal-evaluation/src/lib.rs`
- `crates/temporal-evaluation/src/manifest.rs`
- `crates/temporal-evaluation/src/matrix.rs`
- `crates/temporal-evaluation/tests/platform.rs` (new)
- `crates/temporal-evaluation/tests/contracts.rs` (extend)
- `src/app/live_evaluation.rs`
- `src/app/platform_evidence.rs` (new, test-only)
- `src/app.rs`

The lane registry is explicit and ordered:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformLaneId {
    LinuxStableChromeReferenceHost,
    MacosChromeDefaultDpi,
    MacosChromeHighDpi,
    LinuxChromiumOptional,
}

impl PlatformLaneId {
    pub const ALL: [Self; 4] = [
        Self::LinuxStableChromeReferenceHost,
        Self::MacosChromeDefaultDpi,
        Self::MacosChromeHighDpi,
        Self::LinuxChromiumOptional,
    ];

    pub const REQUIRED: [Self; 3] = [
        Self::LinuxStableChromeReferenceHost,
        Self::MacosChromeDefaultDpi,
        Self::MacosChromeHighDpi,
    ];

    pub fn definition(self) -> PlatformLaneDefinition;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformLaneDefinition {
    pub lane: PlatformLaneId,
    pub required: bool,
    pub platform: Platform,
    pub browser_product: BrowserProduct,
    pub requested_device_scale_factor: u16,
    pub minimum_observed_device_scale_factor: u16,
    pub maximum_observed_device_scale_factor: Option<u16>,
    pub reference_host: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformLaneEvidence {
    pub lane: PlatformLaneId,
    pub run_manifest_digest: String,
    pub run_input_digest: String,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformMatrixRecord {
    pub schema_version: u16,
    pub kind: String,
    pub benchmark_id: String,
    pub lanes: Vec<PlatformLaneEvidence>,
    pub reference_host_status: EvaluationStatus,
    pub cross_platform_status: EvaluationStatus,
    pub optional_linux_chromium_status: EvaluationStatus,
    pub status: EvaluationStatus,
    pub non_claims: Vec<String>,
    pub failure: Option<FailureRecord>,
}

pub fn validate_platform_lane(
    lane: PlatformLaneId,
    manifest: &RunManifest,
) -> Result<()>;

pub fn aggregate_platform_matrix(
    lane_manifests: &[(PlatformLaneId, RunManifest)],
) -> Result<PlatformMatrixRecord>;
```

`PlatformLaneDefinition` requires exact environment and browser identity, the existing canonical
fixture/benchmark digest, the declared 800x450 viewport, the lane's scale band, the live
qualification profile, and a complete/non-placeholder manifest status. The high-DPI definition
requires an observed scale of at least 1.5 (represented in the existing integer thousandths
contract); it does not accept wrapper intent as evidence. `PlatformLaneEvidence` retains only
manifest and input digests plus status/failure because all exact measurements remain in the
referenced `RunManifest`.

Extend the existing current-v1 manifest contract only where necessary to carry a declared
`PlatformLaneId` for platform runs and to permit the high-DPI scale band under the existing typed
live qualification block. Keep the default live qualification profile valid for its existing
scale-one contract; platform runs use a registered `platform-evidence-v1` profile or an equally
explicit registry entry, not an unvalidated threshold-profile string. Generated schema/sample
artifacts are regenerated through the existing generators. No legacy reader or compatibility
alias is introduced.

`PlatformLaneConfig` wraps the existing `LiveQualificationConfig` with a lane definition and
expected wrapper/DPI mode. It passes a test-owned output root, browser product, run ID, and
operator decision into the same production composition. It never invokes a model or product CLI.

**Acceptance criteria**:

- [ ] The lane registry has exactly four entries in canonical order, three required entries, and
      one optional Linux Chromium entry; all runner and validator decisions derive from it.
- [ ] A platform manifest records exact browser/protocol, environment/architecture,
      Rust/toolchain/Git, fixture digest, capture configuration, seed, thresholds, viewport,
      observed scale, gaps, retention, and non-claims through the existing `RunManifest` fields.
- [ ] A high-DPI wrapper request with observed scale one is rejected as decisive evidence; no
      passing high-DPI document or cross-platform claim is emitted.
- [ ] Contract tests reject a wrong browser/product, platform, fixture digest, viewport, scale
      band, profile, input digest, or lane/status pairing.
- [ ] Disabled/unauthorized execution has no browser, server, profile, store, model, or output
      side effect; operator authorization and local Chrome availability remain explicit blockers.

### Unit 2: Linux stable-Chrome reference-host evidence

**Files**:

- `src/app/platform_evidence.rs`
- `src/app/live_evaluation.rs`
- `crates/temporal-evaluation/tests/platform.rs`
- `docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub async fn run_linux_reference_host(
    config: PlatformLaneConfig,
) -> Result<RunManifest>;
```

The lane requires an operator-authorized Linux host, stable Chrome selected explicitly from the
installation registry, the canonical 800x450/default-DPI capture profile, and the live harness's
full qualification output. The final manifest must identify the observed stable-Chrome
Browser.getVersion/protocol identity and exact revision/environment, not merely the requested
binary name. A missing installation or failed preflight is `Blocked`; an incomplete run is
`Inconclusive`; complete below-threshold evidence is `Fail`; only complete passing evidence is
usable as the reference-host checkpoint.

This is the named evidence checkpoint consumed by manual interpretation. Its acceptance is about
one declared live manifest, not about macOS or the optional Chromium lane. The story remains
incomplete until the operator supplies the run or records the explicit blocker; design and
ordinary verification do not run Chrome.

**Acceptance criteria**:

- [ ] One canonical Linux stable-Chrome live manifest is produced only after explicit operator
      authorization and validates as `Pass` with its exact source interval, gaps, cleanup, browser,
      platform, scale, and qualification measurements.
- [ ] The manifest is independently addressable by its digest and its ignored run output remains
      outside Git; no source frames, artifacts, transcripts, or private paths are committed.
- [ ] Linux Chrome absence, unsupported protocol, capture gaps, retention loss, cleanup failure,
      and incomplete matrix coverage remain explicit non-passing states with recovery actions.
- [ ] The checkpoint's output is sufficient for the manual interpretation dependency without
      claiming macOS, high-DPI, Linux Chromium, model comprehension, or cross-platform support.

### Unit 3: macOS stable-Chrome default-DPI evidence

**Files**:

- `src/app/platform_evidence.rs`
- `crates/krometrail-cdp/src/qualification_support/`
- `crates/temporal-evaluation/tests/platform.rs`
- `docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub async fn run_macos_default_dpi(
    config: PlatformLaneConfig,
) -> Result<RunManifest>;
```

The lane independently requires macOS, stable Chrome, the canonical 800x450 viewport, and
observed device scale one. It uses the existing default-DPI wrapper variant and the same
production live qualification authority as Linux. The wrapper variant is configuration, not
proof: the manifest validator accepts the row only after Chrome reports the expected environment,
browser, viewport, and scale.

This lane is independent of the Linux reference-host story and the macOS high-DPI story in the
implementation graph. Its output is consumed by the final matrix aggregator; its absence does not
block Linux reference-host interpretation or agent debugging.

**Acceptance criteria**:

- [ ] A decisive row requires observed macOS stable Chrome, default-DPI scale one, canonical
      viewport, complete live qualification, and exact manifest identity.
- [ ] Non-macOS execution, unavailable Chrome, wrong scale, failed capture, or incomplete cleanup
      produces an explicit blocked/inconclusive result and no macOS pass.
- [ ] The row records the actual macOS version class, architecture, Chrome/protocol identity,
      Rust/toolchain/Git, fixture/capture/seed/threshold identities, and non-claims through the
      existing manifest authority.
- [ ] No default-DPI row is inferred from the committed historical smoke file; that file remains
      prerequisite context only.

### Unit 4: macOS stable-Chrome high-DPI evidence

**Files**:

- `src/app/platform_evidence.rs`
- `src/app/live_evaluation.rs`
- `crates/krometrail-cdp/src/qualification_support/`
- `crates/temporal-evaluation/tests/platform.rs`
- `docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub async fn run_macos_high_dpi(
    config: PlatformLaneConfig,
) -> Result<RunManifest>;
```

The high-DPI lane is a separate macOS stable-Chrome run, not a relabeling of default-DPI output.
It requests the existing high-DPI wrapper variant and requires production capture metadata to
observe the declared scale band (minimum 1.5, canonical requested value 2.0). The current smoke
history is used only to explain why this lane is still absent: prior flags observed scale one and
therefore did not produce a passing document.

Default-DPI and high-DPI runs may be scheduled as independent lane work, but each run has a
sequential contract: preflight, wrapper launch, observed scale/viewport barrier, live
qualification, manifest validation, cleanup, then publication. The existing real-browser lock
prevents unsafe concurrent launches.

**Acceptance criteria**:

- [ ] A high-DPI pass requires macOS stable Chrome and observed device scale at least 1.5 in the
      production capture metadata, not only wrapper arguments.
- [ ] Observed scale one, unavailable macOS/Chrome, unsupported protocol, capture gaps, or cleanup
      failure remains blocked/inconclusive and emits no passing high-DPI evidence.
- [ ] The high-DPI manifest preserves exact source/observed/session timing, viewport/scale,
      browser/environment, capture configuration, fixture, seed, threshold, and non-claim facts.
- [ ] The lane never blocks the Linux reference-host checkpoint or manual interpretation; a missing
      high-DPI row makes only the cross-platform assessment inconclusive.

### Unit 5: optional Linux Chromium evidence

**Files**:

- `src/app/platform_evidence.rs`
- `crates/temporal-evaluation/tests/platform.rs`
- `docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub async fn run_linux_chromium_optional(
    config: PlatformLaneConfig,
) -> Result<RunManifest>;
```

The optional lane selects `BrowserProduct::Chromium` explicitly and runs only on Linux. If the
installation is absent, the lane records `Skipped` with `OptionalUnavailable` and a recovery
instruction. If installed, it must still complete the same live qualification and records its
own product/protocol identity. It is never folded into the Chrome reference-host row, never used
to satisfy macOS coverage, and never silently excluded after a failed observed run.

**Acceptance criteria**:

- [ ] Absent Linux Chromium is an explicit optional skip, not a failure and not a pass; every row
      in the optional run is skipped with its own failure record.
- [ ] An observed Chromium run is separately identified and fully validated; complete failures are
      `Fail` and incomplete evidence is `Inconclusive`.
- [ ] Chromium output cannot satisfy the Linux stable-Chrome reference-host dependency or any
      cross-platform threshold.
- [ ] The lane remains opt-in and does not add a browser download, network fallback, or product
      command.

### Unit 6: matrix aggregation and publication policy

**Files**:

- `crates/temporal-evaluation/src/platform.rs`
- `crates/temporal-evaluation/src/lib.rs`
- `crates/temporal-evaluation/tests/platform.rs`
- `src/app/platform_evidence.rs`
- `docs/evidence/temporal-evaluation/v1/README.md`

```rust
pub struct PlatformMatrixInputs<'a> {
    pub required_lanes: &'a [(PlatformLaneId, RunManifest)],
    pub optional_lanes: &'a [(PlatformLaneId, RunManifest)],
}

pub fn build_platform_matrix(
    inputs: PlatformMatrixInputs<'_>,
) -> Result<PlatformMatrixRecord>;
```

The aggregator requires exactly one record per lane identity, validates each manifest through the
lane registry, and preserves ordered lane records. It reports separate assessment statuses:

- `reference_host_status` is based only on Linux stable Chrome;
- `cross_platform_status` requires Linux stable Chrome, macOS default-DPI, and macOS high-DPI;
- `optional_linux_chromium_status` is `Skipped` only for optional absence; and
- top-level `status` is `Pass` only when the required matrix is complete and passing, `Fail` when
  complete required evidence violates a threshold, `Blocked` when a required precondition such as
  Linux Chrome authorization/install is absent, and `Inconclusive` when evidence exists but macOS
  coverage, high-DPI observation, retention, gaps, or sample minimums prevent a decisive matrix
  claim.

A missing macOS row must therefore leave `reference_host_status` eligible for downstream manual
work while leaving `cross_platform_status` and the matrix claim `Inconclusive`. The record's
non-claims explicitly say that no cross-platform claim is made unless all required rows pass and
that no macOS result is inferred from Linux or historical smoke evidence. Aggregation is a
read/validate operation over local manifests; it does not launch a browser, collect a frame, invoke
a model, or publish source payloads.

**Acceptance criteria**:

- [ ] Aggregation is deterministic in registry order and rejects duplicate/missing lane IDs,
      mismatched benchmark/fixture/input digests, wrong platform/product/scale, mixed statuses,
      and forged manifest references.
- [ ] A valid Linux reference row plus absent macOS rows yields a usable `reference_host_status`
      and `Inconclusive` cross-platform/matrix status; it cannot block manual interpretation or
      agent debugging and cannot claim macOS.
- [ ] All three required rows passing is the only route to a `Pass` cross-platform assessment.
- [ ] Complete below-threshold required evidence is `Fail`; unavailable/partial/gapped/evicted or
      unobserved high-DPI evidence is `Blocked`/`Inconclusive`; optional Chromium absence is only
      `Skipped`.
- [ ] The aggregate contains no image bytes, model answers, transcripts, absolute paths, page
      text, credentials, or copied live measurement payloads.
- [ ] Generated platform schemas/samples are byte-stable and do not modify
      `docs/public/llms-full.txt` or `.work/bin/work-view`.

## Implementation order and child checkpoints

The shared lane contract is first. After it is verified, the four lane stories are independent
implementation/evidence lanes and may be advanced in parallel at the substrate level. Actual
browser execution is serialized by the existing real-browser lock and each lane remains
sequential internally: authorize/preflight → launch → observe profile metrics → qualify → validate
manifest → clean up → retain ignored output. The aggregator is last because it consumes all lane
contracts, but an absent optional Chromium row is a valid terminal skip and does not hold the
required matrix hostage.

1. `epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner`
   — depends on `epic-prove-temporal-advantage-live-capture-and-system-qualification`.
2. `epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence`
   — depends on the shared lane contract; this is the exact manual-interpretation prerequisite.
3. `epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-default-dpi-evidence`
   — depends on the shared lane contract and is independent of the Linux/high-DPI checkpoints.
4. `epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-high-dpi-evidence`
   — depends on the shared lane contract and independently proves observed high-DPI metadata.
5. `epic-prove-temporal-advantage-platform-evidence-collection-linux-chromium-optional-evidence`
   — depends on the shared lane contract; optional absence closes as an explicit skip.
6. `epic-prove-temporal-advantage-platform-evidence-collection-matrix-aggregation-and-claim-boundary`
   — depends on all four lane checkpoints and publishes the deterministic matrix assessment.

These are design/evidence checkpoints for one feature owner, not default worker assignments. The
Linux and macOS lanes have distinct external evidence boundaries, while the shared runner and
aggregator have distinct contract boundaries; that is why stories earn a place here.

## Simplification

- Reuse `RunManifest`, `LiveQualification`, `SourceInterval`, existing capture/storage/artifact
  authorities, the lane-independent fixture, and the real-browser lock. Do not add a platform
  browser runtime, a second store, a second artifact/provenance format, or a product command.
- Keep platform aggregation as digest/index metadata. Exact measurements remain in the live
  manifest; raw frames, artifacts, transcripts, logs, and model answers remain ignored outputs.
- Keep lane definitions in one registry. Do not duplicate required/optional status, DPI thresholds,
  browser products, or claim text in runner branches, tests, README examples, or manual features.
- Do not make macOS completion a prerequisite for the Linux reference host. This removes a false
  coupling rather than weakening the cross-platform requirement.

## Testing

- Registry and manifest contract tests protect lane identity, environment/product/viewport/scale
  validation, profile selection, required/optional status, and canonical serialization.
- Runner tests protect no-side-effect opt-out, shared production authority reuse, existing lock
  serialization, per-lane output boundaries, cleanup, and operator authorization without Chrome.
- Linux reference tests protect the exact required evidence gate and ensure a blocked/incomplete
  run cannot satisfy it.
- macOS tests protect independent default/high-DPI requirements and reject the historical
  wrapper-flag-only high-DPI result.
- Aggregation tests protect deterministic ordering, separate reference/cross-platform statuses,
  absent-macOS inconclusive behavior, optional Chromium skip closure, complete-fail behavior, and
  no fabricated platform claims.
- No test launches Chrome, invokes a model, reads a committed smoke artifact as new evidence, or
  compares ignored output as ground truth. One explicitly authorized ignored live test is the
  operator path, as in the completed live-qualification feature.

## Risks

- **Operator and host availability**: Linux stable Chrome and macOS Chrome may not be installed or
  authorized. Linux reference-host absence blocks the dependent manual lane; macOS absence leaves
  only the platform matrix inconclusive. The design records recovery actions and never substitutes
  another host.
- **High-DPI observability**: the prior smoke runs observed scale one despite high-DPI flags. The
  high-DPI lane fails closed on that observation; no wrapper argument or historical file can satisfy
  it.
- **False dependency coupling**: making manual interpretation wait for the platform parent would
  unnecessarily wait for macOS and would turn a valid Linux reference-host result into unavailable
  agent evidence. The exact Linux child dependency is the guard against that coupling.
- **Silent platform claims**: a Linux pass, optional Chromium result, or historical default-DPI
  document cannot be presented as macOS or cross-platform evidence. Registry validation and
  separate assessments enforce the boundary.
- **Shared browser lock and output collisions**: substrate lanes may be independent, but live
  launches are serialized and run IDs/output roots are validated. A lane never shares a profile,
  store, or mutable manifest with another lane.
- **Evidence retention**: an aggregate digest without retained referenced manifests is not a live
  claim. Matrix validation fails or becomes inconclusive when a referenced manifest/source range
  is missing, evicted, corrupt, or incomplete.
- **Overclaiming from a reference host**: a Linux reference run can unblock the named manual model
  lane but does not establish cross-platform parity, all-model generality, or a product-thesis
  result outside the declared model/configuration.

## Operator blockers and non-claims

This design does not collect evidence. Before the operator runs a lane, they must authorize the
real-browser test path, provide the requested local installation and temporary profile/loopback
permissions, retain enough ignored output storage, and accept that cleanup failures remain visible.
Linux stable Chrome is required for the reference-host checkpoint. macOS default-DPI and high-DPI
are required only for a decisive cross-platform assessment and may remain unavailable; no agent
or model evaluation waits on them. Linux Chromium is optional.

A passing lane qualifies only its named browser, OS/architecture, viewport, observed scale,
fixture, capture configuration, revision, and live qualification profile. A passing Linux
reference lane is sufficient environment evidence for the later manual interpretation dependency
but does not claim macOS, high-DPI, Chromium equivalence, model comprehension, debugging uplift,
cross-platform support, or an automatic diagnosis capability.

## Review and implementation notes

- Execution capability: feature-owner design with sequential shared-contract/aggregation checkpoints
  and independent platform evidence lanes; no browser, model, or cross-model review was invoked.
- Review weight: standard integrated feature review after the lane stories and matrix contract are
  implemented; child stories advance directly to `done` after their own verification.
- Design-only scope: no Chrome was launched, no live evidence was collected, no model was invoked,
  no performance review was run, and no `.work/bin/work-view` content was touched.
- The historical cross-platform smoke evidence is cited as prerequisite context only. Its absent
  high-DPI document remains absent until a future run observes the required production scale.
