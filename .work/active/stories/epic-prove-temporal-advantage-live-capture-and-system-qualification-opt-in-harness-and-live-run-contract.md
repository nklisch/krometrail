---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-opt-in-harness-and-live-run-contract
kind: story
stage: done
tags: [testing, infra]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Establish the opt-in live qualification boundary

## Checkpoint

Create the test-only harness boundary and canonical live-qualification manifest profile before
adding any browser scenario. Ordinary workspace tests remain browser-free. The live path requires
both the existing real-browser authorization and `KROMETRAIL_LIVE_CAPTURE_EVALUATION=1`; absent
opt-in means no browser discovery, fixture server, profile, operator store, or live output.

## Exact implementation

Add the shared test-support surface at `crates/krometrail-cdp/src/qualification_support/` and
re-export it from the current CDP integration-test support modules. Promote, rather than copy, the
existing browser identity gate, serialized browser lock, managed `ChromeWrapper`, profile cleanup,
and static fixture serving behavior. The support surface is gated behind a non-default
`qualification-support` feature and has no product CLI/API role. Add viewport-aware wrapper
construction without changing the default smoke wrapper flags; the live default must request
800x450 at device scale one.

Add the test-only composition module at `src/app/live_evaluation.rs`. It must build one temporary
runtime graph through the same `open_storage_with_budget`, `ProductionBrowserConnector`,
`RecordingStore`, `TemporalVisionArtifactService`, `ProgressiveEvidenceService`, and
`TemporalDebugBundleService` authorities used by `build_runtime`. Return a test-owned runtime
handle containing the injected ports and concrete store only for observation. Never call the
operator data-directory/profile defaults.

Extend `crates/temporal-evaluation/src/manifest.rs`, `prompts.rs`, `matrix.rs`, `lib.rs`, and the
contract tests with the prepublic `live-qualification-v1` profile and `CaptureQualification`
prompt identity. Add the typed `LiveQualification`, `QualificationGateId::ALL`, gate result, and
measurement structures described in the feature. Keep the existing `RunManifest` as the only
manifest/result schema; do not add a live result file type or compatibility alias. Extend input
digest exclusion and status validation in place, including explicit failure/recovery text and
privacy-safe non-claims. Update generated schema/sample artifacts only through the existing
contract generator, never by hand.

Add a lifecycle runner with these test-only boundaries:

```rust
pub enum OptInDecision { Disabled, Authorized }
pub enum BrowserPreflight { Ready(BrowserInstallation), Blocked(FailureRecord), Skipped(FailureRecord) }
pub struct QualificationRuntime { /* injected ports + one concrete RecordingStore */ }
pub async fn run_preflight(config: LiveQualificationConfig) -> Result<PreflightResult>;
pub async fn finalize_manifest(run: RunManifest, cleanup: CleanupObservation) -> Result<PathBuf>;
```

The runner must acquire the existing lock before launch, use a loopback server readiness barrier,
create a managed profile below `target/temporal-evaluation/live/`, and perform bounded cleanup in
an idempotent guard. A blocked/ skipped preflight is serialized only when the feature-specific
opt-in was supplied. No raw paths, credentials, page text, image bytes, or environment dumps may
enter the manifest.

## Acceptance evidence

- [x] Default `cargo test --workspace --all-targets --locked` has no path to Chrome, a listener,
      managed profile, operator data directory, model, network, or live output.
- [x] Opt-in is checked before any side effect; required browser absence is `blocked`, optional
      Linux Chromium absence is `skipped`, and both carry safe reason/recovery records.
- [x] Scripted tests prove the runtime graph shares one concrete store across recording, retention,
      timeline, gaps, frames, queries, progressive evidence, and artifact/bundle services.
- [x] Live manifest round trips canonically, requires exactly the registered gates for
      `live-qualification-v1`, rejects unknown/duplicate gate IDs and unsafe paths/text, and
      rejects pass/fail claims whose rows/gates do not support them.
- [x] Cleanup runs after preflight failure and simulated launch/transport/write failures, is safe
      to repeat, and records failure rather than claiming success when a resource remains.
- [x] No browser is launched while implementing or verifying this checkpoint.

## Ordering

This checkpoint unblocks all capture and measurement scenarios. It intentionally has no child
story dependency; the parent feature's benchmark and deterministic-scoring dependencies remain the
feature-level prerequisites.

## Implementation notes

- Execution capability: feature-owning Luna worker, inline implementation; the story's support, contract, and composition seams share one Rust ownership boundary.
- Review weight: standard feature review remains owned by the parent feature; this child checkpoint advanced directly to `done` after verification.
- Files changed: gated CDP qualification support and re-export shims; test-only live composition/lifecycle module; temporal evaluation manifest, prompt, matrix, and registry contracts; generated contract artifacts; locked dependency metadata.
- Tests added: live manifest round-trip, gate registry, status, privacy, input-digest, blocked/skipped, opt-in ordering, lifecycle rejection, shared-store pointer, cleanup, and viewport-wrapper tests.
- Verification: Rust 1.85 locked workspace check, test, and clippy passed; feature-gated root/CDP tests passed; no live environment variables were supplied and no browser was launched.
- Simplification: existing browser support implementations are promoted through the feature-gated source support surface and integration-test re-export shims; smoke wrapper flags remain unchanged.
- Discrepancies from design: later capture scenarios, fixture observation, control matrix, retention/recovery measurements, and operator evidence remain intentionally unimplemented for their dependent child checkpoints.
- Adjacent issues parked: none.
