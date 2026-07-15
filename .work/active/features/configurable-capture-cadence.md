---
id: configurable-capture-cadence
kind: feature
stage: implementing
tags: [browser, agent-ux, visual, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Configurable Capture Cadence

## Brief

Give the person or agent that starts a browser session one explicit, typed control over the relative visual-capture stride. `every_nth_frame` is accepted on both `LaunchBrowser` and `AttachBrowser`, exposed through the generated MCP `start_browser` and `attach_browser` schemas, validated at the external boundary, and frozen for the lifetime of that browser connection/recording session. The value is an integer from 1 through 60 and defaults to 1, preserving maximum-evidence behavior when omitted.

The stride is a best-effort relative sampling request, not an exact frame rate. The CDP adapter passes it directly to CDP `Page.startScreencast.everyNthFrame`. Session and capture status, evaluation manifests, and accepted-claim provenance record the requested stride so reduced capture probability is visible to humans and agents. Deliberate stride selection remains distinct from ordinary queue drops, persistence failures, visibility gaps, and other capture-gap causes.

## Strategic decisions

- **Configuration authority**: capture stride is a per-connection/per-recording-session request; there is no mid-stream mutation or stop/restart path in v1, because changing it would require an explicit capture-gap contract.
- **Public contract**: core launch and attach request types are the single domain contract; MCP lifecycle schemas are generated from those types. No CLI command, environment variable, configuration-file setting, alias, shim, or legacy reader is added for this capability.
- **Semantics**: `every_nth_frame` is a bounded relative stride (1..=60), not an FPS promise. The requested value is provenance, while observed cadence and known loss remain separate evidence.
- **Audience**: agents and humans using an MCP client receive the same field and validation behavior.

## Design decisions

- **Core owner and representation**: `krometrail-core` owns one `EveryNthFrame` transparent newtype backed by a non-zero `u8`. Its constructor, `Default`, `Deserialize`, and generated schema all enforce or advertise the inclusive 1..=60 contract. The request field is named `every_nth_frame`; CDP converts it to the wire spelling `everyNthFrame` only at the adapter boundary.
- **Defaulting boundary**: omission defaults to `EveryNthFrame::default()` (1) in both request types. JSON values 0, 61+, fractional values, strings, and null are rejected before the connector is called. No later adapter validation or fallback can turn an invalid request into a default.
- **Launch and attach symmetry**: `LaunchBrowser` gets the public field directly and retains its existing `#[serde(default)]` behavior. `AttachBrowser` keeps its endpoint-validation wire projection, adds the same typed field to that one wire shape, and makes only the stride optional. `AttachBrowser::new` defaults to 1; a small builder is used for programmatic non-default construction. There is no second attach-specific cadence type.
- **Existing global `CaptureConfig`**: the CDP `CaptureConfig` remains the process/composition-wide operational configuration for format, queue bounds, payload limits, timing, and shutdown. It does not gain a user-facing stride field and does not become a second cadence authority. `ProductionBrowserConnector::connect` extracts the validated request value once and passes it to the session-owned `CaptureCoordinator`, which stores it immutably and supplies it to every stream generation.
- **Status shape**: add `every_nth_frame: EveryNthFrame` directly to `BrowserStatus` (session projection) and `TargetCaptureStatus` (standalone capture/event projection). Repeating the immutable scalar in these read-only projections avoids changing `capture: Vec<TargetCaptureStatus>` into a wrapper object and lets a `CaptureStateChanged` event carry provenance without a follow-up status call. It is not a mutable setting and does not add a status registry.
- **Recording catalog**: `RecordingSession` stores the same typed value and exposes a getter. Its constructor requires the value so a persisted session cannot silently lose the requested stride. Existing store/test constructors are updated directly; no optional compatibility reader or migration shim is added in this unreleased repository.
- **Reconnect invariant**: the session-owned coordinator survives transport replacement. `StartCapture` and `ResumeCapture` effects continue to carry only target/connection/attachment/session transport identity; they never carry a replacement cadence. Every reconnect generation therefore sends the original value, while stale old-generation callbacks retain the existing fences and cannot start a stream.
- **Evaluation identity and claims**: add `every_nth_frame: u8` to the existing evaluation `CaptureConfigIdentity`, with the same 1..=60 validation and default sample value. This evaluation crate remains independent of core, so the manifest's scalar is a serialized identity projection, not a second runtime type or authority. `AcceptedClaim` remains only claim ID plus evidence IDs: its containing manifest's `krometrail.capture_config` and the result's `run_manifest_input_digest` bind the claim to the requested stride without copying cadence into every claim.
- **Generated contracts**: MCP lifecycle schemas are generated at route construction from `LaunchBrowser` and `AttachBrowser`; there is no checked-in MCP schema to hand-edit or regenerate. The checked-in evaluation sample/schema remain generated artifacts and are regenerated with the existing `generate-run-manifest` binary after the manifest contract changes.
- **Foundation state**: `docs/SPEC.md` and `docs/ARCHITECTURE.md` already describe this intended contract from the preceding scope change. No foundation document or generated artifact is changed by this design-only commit; implementation must re-check those assertions and update only if code reveals a contradiction.

## Architectural choice

Three approaches were considered:

1. **Put `every_nth_frame` into CDP `CaptureConfig`**. This would make the wire change small, but would blur process-wide operational limits with a per-session request and make `with_capture(CaptureConfig)` look like an alternative public cadence setting. It also invites reconnect code to clone or replace the wrong configuration. Rejected.
2. **Introduce a new public `SessionCaptureConfiguration` object and pass it through every layer**. This would make future session settings easy to add, but one field does not yet earn another public object, schema, or serialization layer. It adds a second shape beside the existing request/status/manifest authorities. Rejected.
3. **Use one core `EveryNthFrame` value, bind it once in the production session, and project it into the existing capture/status/evaluation authorities** (chosen). The request owns boundary validation, `SessionShared` owns the session projection, and `CaptureCoordinator` is the sole capture consumer for all targets and reconnect generations. This keeps the public change small while preserving strict ownership; the same immutable scalar appears in projections only because events and manifests are independently consumable.

## Tricky unit first: immutable session capture authority

The highest-risk unit is not the integer validator or the MCP field. It is preventing the existing supervisor's attachment/reconnect machinery from accidentally reselecting the cadence or treating deliberate sampling as loss:

```text
LaunchBrowser / AttachBrowser
        │  validated EveryNthFrame
        ▼
ProductionBrowserConnector::connect
        │  bind once to the new session
        ▼
SessionShared + CaptureCoordinator (immutable request value)
        │
        ├── every initial/dynamic target generation
        ├── every reconnect ResumeCapture generation
        ├── BrowserStatus / TargetCaptureStatus projections
        └── evaluation CaptureConfigIdentity
```

`CaptureCoordinator` remains alive for the browser session, while `CaptureAssembly.config` remains the existing process/composition defaults. `StreamRuntime` receives a copied `EveryNthFrame` only as an immutable execution input. The CDP start-parameter builder inserts one `everyNthFrame` property for both JPEG and PNG paths; it does not alter acknowledgement, ordinal, queue, persistence, gap, or observed-cadence logic. A scripted transport test must observe the same value before and after a physical transport replacement and must observe no deliberate-stride gap.

## Implementation units

### Unit 1: core typed request, recording, and status contracts

**Story**: `configurable-capture-cadence-core-contracts-and-status`

**Files**:

- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/browser/control.rs`
- `crates/krometrail-core/src/ports/{browser.rs,mod.rs}` tests and existing core contract tests
- store tests that construct `RecordingSession`

Representative boundary:

```rust
pub const MIN_EVERY_NTH_FRAME: u8 = 1;
pub const MAX_EVERY_NTH_FRAME: u8 = 60;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct EveryNthFrame(NonZeroU8);

impl EveryNthFrame {
    pub fn new(value: u8) -> Result<Self>;
    pub const fn get(self) -> u8;
}

impl Default for EveryNthFrame { /* value 1 */ }
impl<'de> Deserialize<'de> for EveryNthFrame { /* deserialize then call new */ }

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct LaunchBrowser {
    pub executable: Option<PathBuf>,
    pub profile: ManagedProfile,
    pub initial_url: Option<String>,
    pub every_nth_frame: EveryNthFrame,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AttachBrowser {
    pub endpoint: String,
    pub every_nth_frame: EveryNthFrame,
}

impl AttachBrowser {
    pub fn new(endpoint: impl Into<String>) -> Result<Self>;
    pub fn with_every_nth_frame(self, every_nth_frame: EveryNthFrame) -> Self;
}
```

Use the existing validated-transparent-newtype pattern (`EventPageLimit`) for serde and error
mapping. The generated schema must publish an integer property with minimum 1, maximum 60, and
an optional/default-1 field in both request object shapes; if the derive cannot express the range,
add the smallest local schema override on `EveryNthFrame`, not a second wire type.

Add `EveryNthFrame` to `RecordingSession` and to both status projections:

```rust
pub struct RecordingSession { /* existing fields */, every_nth_frame: EveryNthFrame }
pub struct BrowserStatus { /* existing fields */, pub every_nth_frame: EveryNthFrame }
pub struct TargetCaptureStatus { /* existing fields */, every_nth_frame: EveryNthFrame }

impl RecordingSession {
    pub fn new(/* existing args */, every_nth_frame: EveryNthFrame, /* existing args */) -> Result<Self>;
    pub const fn every_nth_frame(&self) -> EveryNthFrame;
}
```

The status and recording custom wire projections include the same typed field and validate it on
read. Their constructors require the field; no serde fallback is added to persisted status/session
records. Update existing test and store fixtures with `EveryNthFrame::default()` explicitly where
they are not testing a non-default request.

**Acceptance criteria**:

- [ ] `EveryNthFrame::new` accepts exactly 1 and 60, rejects 0 and 61, and JSON deserialization rejects non-integer/null/out-of-range values with the normal invalid-input path.
- [ ] Omitted `every_nth_frame` defaults to 1 for both `LaunchBrowser` and `AttachBrowser`; a non-default value round-trips through each request's JSON shape.
- [ ] `BrowserStatus`, `TargetCaptureStatus`, and `RecordingSession` serialize, deserialize, and preserve the requested typed value; no FPS/frame-rate field or inferred gap field appears.
- [ ] Existing status-selection, statistics, lifecycle, and recording invariants remain unchanged apart from the required cadence field.
- [ ] All existing `RecordingSession` store/catalog contract fixtures compile with explicit default values; no compatibility reader or data migration is introduced.

### Unit 2: CDP session binding, forwarding, and reconnect behavior

**Story**: `configurable-capture-cadence-session-capture-forwarding`

**Files**:

- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`
- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`
- `crates/krometrail-cdp/src/session/reconnect.rs` only if an invariant/test seam requires it
- `crates/krometrail-cdp/tests/session_capture.rs`
- `crates/krometrail-cdp/tests/session_supervision.rs`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- existing CDP capture/evidence fixtures that serialize effective configuration

Keep `CaptureConfig` unchanged as the global operational assembly. Extend the private capture
authority rather than the global config:

```rust
pub(crate) struct CaptureCoordinator {
    config: CaptureConfig,
    every_nth_frame: EveryNthFrame,
    /* existing dependencies, observer, stream registry, ordinal registry */
}

impl CaptureCoordinator {
    pub(crate) fn new(
        config: CaptureConfig,
        every_nth_frame: EveryNthFrame,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
    ) -> Result<Self, CaptureError>;
    pub(crate) fn every_nth_frame(&self) -> EveryNthFrame;
}
```

`StreamRuntime` receives the copied value. Build format-specific parameters first, then insert the
single shared wire property:

```rust
let mut params = format_parameters(&runtime.config);
params["everyNthFrame"] = json!(runtime.every_nth_frame.get());
```

`ProductionBrowserConnector::connect` reads the request value before consuming the launch/attach
enum, stores it in the immutable session projection, and passes it to the one coordinator. The
same `Arc<CaptureRuntime>` is reused by `apply_effects` for initial, dynamic, suspended, resumed,
and reconnect generations. `CaptureEffectContext` remains transport/target identity only; it must
not grow a cadence field or a setter.

`TargetCaptureStatus` is constructed from the coordinator's value on every target status emission.
`BrowserStatus` reads the same request-bound value from the session owner. If a test constructs a
connector without capture assembly, status still reports the request-bound value while the capture
list remains empty.

**Acceptance criteria**:

- [ ] A scripted CDP `Page.startScreencast` call receives the requested `everyNthFrame` value for both JPEG and PNG capture paths; the hardcoded `1` appears nowhere in the production start-parameter builder.
- [ ] Initial, dynamically attached, and resumed target streams all use the exact session value; no target or reconnect input can supply a replacement.
- [ ] A scripted transport disconnect/reconnect sends the same `everyNthFrame` value on the new physical connection, preserves target identity/generation rules, and does not create a stride-derived `CaptureGap`.
- [ ] Existing ack-first ordering, capture ordinals, frame cadence measurements, queue accounting, persistence failures, visibility gaps, and explicit gap reasons are byte-for-byte semantically unchanged.
- [ ] The capture/status event path exposes the requested stride while still reporting observed cadence separately and making no exact FPS claim.
- [ ] The scripted test uses no real Chrome, network, browser process, or sleep-based timing assumption; it observes command parameters and existing reconnect events through the test transport.

### Unit 3: MCP lifecycle schema and boundary forwarding

**Story**: `configurable-capture-cadence-mcp-generated-contracts`

**Files**:

- `crates/krometrail-mcp/src/registry.rs`
- `crates/krometrail-mcp/src/schema.rs` only for focused schema assertions if needed
- `crates/krometrail-mcp/src/session.rs`
- `crates/krometrail-mcp/tests/` lifecycle/schema tests if the existing test target is extended
- core request schema tests from Unit 1

The existing lifecycle route remains the authority:

```rust
LifecycleKind::Start  => type_input_schema::<LaunchBrowser>()?,
LifecycleKind::Attach => type_input_schema::<AttachBrowser>()?,
```

Do not add MCP-local request structs, aliases, validators, environment parsing, or a schema file.
`parse_arguments::<LaunchBrowser>` and `parse_arguments::<AttachBrowser>` must invoke the core
newtype deserializer before `BrowserSessionOwner::start/attach`; valid values are forwarded without
rewriting and invalid values become the existing visible `InvalidInput` response.

**Acceptance criteria**:

- [ ] Generated `start_browser` and `attach_browser` input schemas each contain the same optional `every_nth_frame` integer property with range 1..=60 and default 1; neither schema requires it when omitted.
- [ ] Invalid MCP JSON values are rejected before a connector call, while valid non-default values reach the same core `LaunchBrowser`/`AttachBrowser` request type.
- [ ] Browser status returned by `start_browser`, `attach_browser`, and `browser_status` contains the request-bound stride, and a capture-state event carries the same value.
- [ ] No checked-in MCP schema artifact, parallel lifecycle type, capability/operation registry entry, CLI setting, environment variable, or configuration-file setting is introduced.
- [ ] Schema tests prove the generated shapes rather than asserting on a hand-written JSON copy.

### Unit 4: evaluation identity, accepted-claim traceability, and generated artifacts

**Story**: `configurable-capture-cadence-evaluation-provenance-and-schema`

**Files**:

- `crates/temporal-evaluation/src/manifest.rs`
- `crates/temporal-evaluation/tests/manifest.rs`
- `crates/temporal-evaluation/tests/live_qualification.rs`
- `src/app/live_evaluation/report.rs`
- `src/app/live_evaluation.rs`
- `src/app/live_evaluation/capture.rs` request literals and effective-configuration observations
- `docs/evidence/temporal-evaluation/v1/sample-manifest.json`
- `docs/evidence/temporal-evaluation/v1/run-manifest.schema.json`

Extend the existing identity without creating a second evaluation capture configuration:

```rust
pub struct CaptureConfigIdentity {
    pub every_nth_frame: u8, // validated as 1..=60 in this independent manifest crate
    pub queue_capacity: u16,
    pub max_active_streams: u16,
    pub ack_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
}
```

`validate_krometrail` rejects 0 and values above 60. The canonical sample and contract seed use 1.
The live qualification path launches with the default request and, after a successful connection,
projects the observed `BrowserStatus.every_nth_frame` into the manifest identity. A failed/no-session
qualification remains explicit non-passing evidence and may retain the contract default because no
session value was observed. The value is included in the canonical manifest and therefore in
`RunManifest::input_digest`; `EvaluationResultRecord.run_manifest_input_digest` remains the single
binding from result/accepted claims to that identity.

Do not add `every_nth_frame` to `AcceptedClaim`, `TrialScore`, or the result schema. A claim's evidence
IDs already resolve to the manifest's source/artifact identity, and the enclosing manifest/result
input digest supplies the requested stride exactly once. Add tests that change only the stride and
prove the manifest/input digest changes while retained evidence and accepted-claim validation still
behave under the existing rules.

Regenerate the two checked-in evaluation artifacts with the existing generator after code changes:

```text
cargo run -p temporal-evaluation --bin generate-run-manifest -- \
  docs/evidence/temporal-evaluation/v1/sample-manifest.json \
  docs/evidence/temporal-evaluation/v1/run-manifest.schema.json
```

The implementation must run the manifest/schema byte-stability test and the bounded Rust quality
gates. This design commit does not run the generator or modify those artifacts.

**Acceptance criteria**:

- [ ] Evaluation manifests serialize and validate requested strides 1 and 60 and reject 0/61; the generated schema exposes the same bound.
- [ ] The live qualification report records the request-bound status stride in `KrometrailIdentity.capture_config`, not an unrelated global or inferred cadence.
- [ ] Changing only the requested stride changes canonical manifest and input digests; accepted claims remain traceable through retained evidence and the enclosing manifest identity, without a per-claim cadence copy.
- [ ] The committed sample manifest and generated schema are regenerated through the existing binary and pass their byte-equality contract tests.
- [ ] No evaluation row infers gaps, missing frames, or FPS from the deliberate stride; ordinary gaps and observed cadence remain their existing independent fields.

## Implementation order and child checkpoints

These are sequential design checkpoints for one feature owner, not default worker assignments. Each
checkpoint has a distinct stable boundary and its own black-box evidence, but the parent feature
should normally be implemented and reviewed as one cohesive bundle.

1. `configurable-capture-cadence-core-contracts-and-status` — typed core request, recording-session persistence, status projections, and boundary tests; no CDP dependency.
2. `configurable-capture-cadence-session-capture-forwarding` — depends on step 1; immutable session binding, start-parameter forwarding, and scripted reconnect proof.
3. `configurable-capture-cadence-mcp-generated-contracts` — depends on step 2; generated lifecycle schemas, parsing, and public status forwarding.
4. `configurable-capture-cadence-evaluation-provenance-and-schema` — depends on step 3; manifest identity/claim binding, live-report projection, and checked-in schema/sample regeneration.

The feature is not complete until all four checkpoints are verified. Child stories advance directly from
`implementing` to `done` after their focused evidence; only the feature receives integrated review.

## Simplification

- Keep the new concept to one core `EveryNthFrame` value. Do not add a cadence service, registry,
  capability, `SessionCaptureConfiguration` wrapper, MCP request type, evaluation cadence type, or
  compatibility alias.
- Leave CDP `CaptureConfig` as the existing global operational authority. The request-bound value is
  copied once into the session-owned `CaptureCoordinator`; no CLI/env/file parallel setting is added.
- Replace both hardcoded `everyNthFrame: 1` literals with one shared parameter insertion after
  format-specific parameters. This also removes duplicated CDP wire construction.
- Project the scalar into existing status and manifest identities rather than changing the capture
  list shape, adding a status registry, or duplicating it on every accepted claim/result.
- Do not change queue accounting, ordinals, source/observed/session clocks, gap reasons, frame
  cadence measurements, retention behavior, or the existing generated-artifact ownership.

## Testing

- **Core black-box contracts** protect defaulting, exact inclusive bounds, malformed external JSON,
  typed request round trips, recording-session serialization, status serialization, and the absence
  of FPS/gap inference. They test public core values rather than private field layout.
- **CDP scripted contracts** protect the actual `Page.startScreencast` wire parameter for JPEG/PNG,
  subscribe/start ordering, initial and reconnect generations, unchanged ack/gap semantics, and
  status/event provenance. They use `ScriptedCdp`/existing reconnect factories only; no Chrome or
  network is involved.
- **MCP schema contracts** protect that both lifecycle routes derive the same optional bounded field
  and that malformed values fail before connector invocation. They inspect generated schemas and do
  not maintain a snapshot copy.
- **Evaluation contracts** protect manifest bounds, canonical sample/schema regeneration, live report
  projection, digest identity, accepted-claim retained-evidence validation, and explicit separation
  between requested stride, observed cadence, and declared gaps.
- Existing tests that only use the default stride are updated with explicit `EveryNthFrame::default()`
  where a constructor now requires the new field; no test is deleted merely to avoid updating it.
  No real-browser, model, network, CLI, or environment/config-file test is added.

## Risks

- **Schema constraint drift**: a transparent newtype can serialize correctly while a generated schema
  omits its range/default. The core/MCP schema assertions are a release blocker; use a local schema
  override on the one type if derive attributes do not publish the bounds.
- **Global/session confusion**: adding the field to CDP `CaptureConfig` or `CaptureEffectContext`
  would create a second authority. The chosen composition keeps the global operational config and
  transport target identity separate and tests equality across status projections.
- **Reconnect regression**: a fresh coordinator or request reconstruction could silently revert to 1
  after a disconnect. The scripted two-physical-connection test requires the requested value on
  both start commands and checks the existing target/generation continuity.
- **Overclaiming reduced sampling**: a higher stride can lower evidence probability, but it does not
  prove a gap or an FPS. Existing gap/cadence contracts remain untouched and the manifest identity
  makes deliberate selection visible to evaluators.
- **Generated evaluation drift**: adding a required manifest identity field changes the canonical
  sample/schema. The existing generator and byte-stability test are the only regeneration path; a
  design-only commit intentionally leaves generated files unchanged until implementation.

## Review and implementation notes

- **Dispatch**: direct-read only. The requested surfaces are known from the existing contracts,
  capture hardcode, session reducer, generated schema route, and evaluation generator. No
  exploratory agent, peer review, Chrome, model, network, or implementation command was used.
- **Review weight**: standard integrated feature review after all child checkpoints; design-time
  advisory review is not required for this bounded prepublic contract change.
- **Scope guard**: implementation must not touch `.work/bin/work-view`, add a product command, edit
  `docs/public/llms-full.txt`, or regenerate evaluation artifacts in this design commit.
