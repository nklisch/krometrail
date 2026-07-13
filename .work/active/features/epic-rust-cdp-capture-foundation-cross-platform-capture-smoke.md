---
id: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke
kind: feature
stage: implementing
tags: [browser, testing, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Cross-Platform Capture Fidelity Smoke

## Brief

Provide a minimal real-browser proof that the foundation captures visible transitions with trustworthy timing and loss reporting on supported systems. The smoke exercises current stable Chrome or Chromium on Linux and current stable Chrome on macOS, including a high-DPI configuration, and records browser/protocol identity, frame cadence, distinct clocks, sequence continuity, declared gaps, and shutdown behavior.

Keep this proof intentionally smaller than the evaluation program: use only enough deterministic browser behavior to catch transport, scaling, acknowledgement, and platform regressions in the live frame stream. Duration sweeps, the full visual-defect corpus, artifact comparisons, storage validation, and agent-effectiveness claims remain owned by `epic-prove-temporal-advantage` and its prerequisites.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: final foundation gate — validates the complete live-capture path after production ingestion lands
- Design decisions inherited: qualify capture against real Chrome and report unsupported protocol behavior explicitly

## Foundation references

- `docs/VISION.md` — Local-First Operation and Success
- `docs/SPEC.md` — Supported Environment, Continuous Visual Capture, and Exclusions
- `docs/ARCHITECTURE.md` — Observability and Technology Decisions
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation, Timing Integrity, and Cross-Platform Evaluation

## Execution policy and grounding

- **Driver:** direct redesign under active autopilot `--all` with no user questions; no subagents were spawned.
- **Effective review weight:** standard. The caller prohibited subdelegation, so no design-time independent advisory ran; this does not block design completion. Feature review remains required after implementation.
- **Dispatch rationale:** direct reads covered the feature brief, parent epic and its decomposition risk note, all five foundation docs, the canonical final5 evidence layout (`docs/evidence/cdp-transport/v2/`), the final5 schema (`schema.json`), the rust-CDP transport skill, the done `bounded-screencast-ingestion` feature and its real-Chrome fidelity story, the done `chrome-target-supervision` feature, the existing `crates/krometrail-cdp/tests/capture_real.rs` harness and its `tests/support/chrome.rs` helpers, the production launcher (`launcher/startup.rs`) and its deliberate no-hardcoded-headless/gpu stance, and the `krometrail-core` capture/browser accessor surface.
- **Rolling Foundation:** additive only. No standing foundation claim is contradicted. `docs/EVALUATION.md` already assigns the duration sweep, defect corpus, and product-thesis thresholds to the evaluation epic; this design honors that boundary rather than re-deriving it.

## Scope and honest non-goals

This feature proves the **production** live-capture path (`ProductionBrowserConnector` + exact cdpkit 0.4.0 adapter + supervised targets + bounded ingestion) on each supported platform/Chrome/DPI configuration. It is the final foundation gate.

**In scope:**
- One managed real-browser capture session per configuration against the existing `tests/fixtures/browser/cdp-transport-gate` fixture, asserting browser/protocol identity, frame cadence *as diagnostics*, three distinct clocks, Krometrail `CaptureOrdinal` continuity, declared-gap honesty, scaling on the high-DPI configuration, and ownership-correct shutdown with leak checks.
- One capacity-one blocked-sink loss-reporting session per configuration, scoped to “does explicit `IngestionQueueSaturated` loss survive on this platform/Chrome” — not the full saturation/incomplete-stop/reconnect gauntlet.
- A small `CrossPlatformSmokeEvidence` schema and serializer, distinct from final5’s `TransportEvidenceV2`, recording provenance, per-session measurements, shutdown outcome, leak-check result, and an explicit non-claims list.
- Deterministic no-Chrome tests for the schema, the high-DPI wrapper variant, the per-platform skip logic, and the leak-check helper invariants.

**Honest non-goals (recorded in every evidence document’s `non_claims`):**
- No transport requalification. final5 (`docs/evidence/cdp-transport/v2/`) owns cdpkit selection, 60 s/1000-frame sustained saturation, deterministic wire assertions, and ack p99/max thresholds. The smoke reuses none of those thresholds.
- No product-thesis thresholds. The 100 ms/95 % and 50 ms/80 % capture envelope and the 25 pp agent-improvement bar belong to `epic-prove-temporal-advantage` and are not release-blocking here.
- No duration sweep, visual-defect corpus, artifact comparison, storage validation, or agent-effectiveness measurement (evaluation epic).
- No host-speed percentile assertion. Ack/cadence histograms are recorded as diagnostics with internal-consistency checks only (samples ≥ N, percentiles monotonic); the smoke asserts no `p99 ≤ X ms`.
- No claim that Chrome’s acknowledgement token changes, orders frames, or detects skipped browser frames (already removed from the codebase).
- No re-proof of the saturation/incomplete-stop/reconnect contracts per platform. Those are owned by `crates/krometrail-cdp/tests/capture_real.rs` and are not platform-sensitive in their contracts.

## Design decisions

- **Evidence schema is distinct from final5.** The smoke produces *production-path platform evidence*, not *transport-selection qualification*. Reusing `TransportEvidenceV2` would imply the smoke re-qualifies the transport and would couple two evidence kinds that have different authorities. A new `CrossPlatformSmokeEvidence` schema lives under its own versioned directory (`docs/evidence/cross-platform-smoke/v1/`) with field names chosen so no consumer can mistake one for the other.
- **No source-attestation sha256 tree.** final5 needed a source-file digest tree because it was the *selection* evidence for an external dependency and had to be independently reproducible from source. The smoke’s authority is “the production path on this Chrome version on this platform produced these observed measurements,” not a re-derivation from source. Provenance records git revision, Rust toolchain, cdpkit version, discovered installation, runtime `BrowserVersion`, fixture digests, and launch config — proportionate, not maximalist.
- **Evidence is written by the test, committed by an operator.** The real-Chrome smoke writes `<config>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default: a unique temp path). The real-evidence story runs the smoke with that dir pointed at `docs/evidence/cross-platform-smoke/v1/` and commits the four JSON documents. The test never mutates the source tree during `cargo test`, mirroring how final5 evidence is gathered out-of-band and then committed.
- **High-DPI is macOS-scoped per the brief.** `current stable Chrome on macOS, including a high-DPI configuration` places the high-DPI configuration on macOS. The harness `ChromeTestWrapper::HighDpi` variant works on Linux too, but the smoke does not run a Linux high-DPI configuration because the brief does not ask for one. This keeps the configuration count at four.
- **Linux Chromium is best-effort.** The brief says “Chrome or Chromium” on Linux. The smoke discovers both, runs Chrome always and Chromium when installed, and records `not_installed` as an honest skip when Chromium is absent — never a failure.
- **Always-headless via the shared wrapper.** CI runners have no display. The smoke launches every configuration through `ChromeTestWrapper` (headless), exactly as `capture_real.rs` does, so the smoke is CI-runnable on both lanes without `xvfb` or a macOS display. The production launcher remains unchanged and still does not hardcode headless/gpu/sandbox (its existing test guards this).
- **Loss-reporting session is one blocked-sink run, not the full gauntlet.** Per-configuration loss confidence is “`IngestionQueueSaturated` is declared honestly with a positive missing estimate and target-owned identity, and releasing the sink drains accepted work before a bounded managed stop.” That is the smallest honest loss-reporting proof; the incomplete-stop and proxy-sever reconnect proofs stay in `capture_real.rs`.
- **No visibility forcing per platform.** Real Chrome on these runners emits no `Page.screencastVisibilityChanged` transition (the same honest limitation `capture_real.rs` records). The smoke records the visibility-event count per run and infers no hidden-silence gap; if Chrome does emit a transition, the smoke requires a target-owned `TargetHidden` gap and same-target recovery, reusing the existing conditional visibility assertion.

## Architectural choice

### Option 1: Extend `capture_real.rs` with platform-tagged cases

Add `#[cfg(target_os = …)]` cases and a high-DPI variant directly to the existing real-Chrome test file. Smallest diff. Rejected: `capture_real.rs` is owned by a done story and its four scenarios are transport/ingestion contracts, not platform evidence. Mixing the platform smoke into it blurs ownership, makes the done story’s file the de-facto growth point for a different kind of evidence, and couples the smoke to a file the real-Chrome fidelity story explicitly owns.

### Option 2: A new production command that captures and writes evidence

Ship a `krometrail capture-smoke` subcommand that runs the configurations and writes evidence JSON. Rejected: there is no product command surface for capture yet (`src/cli.rs` exposes only `--version`, `--help`, and `doctor`). Inventing a command to host test evidence violates the project rule against adding command examples for capabilities not present in `src/cli.rs` and pulls evidence gathering into the product runtime.

### Option 3 (chosen): Test-only smoke harness + distinct evidence schema in `krometrail-cdp`, two child stories

A new opt-in test file `crates/krometrail-cdp/tests/cross_platform_smoke.rs` plus a small `tests/support/smoke_evidence.rs` serializer and a committed `docs/evidence/cross-platform-smoke/v1/` schema/README. The harness reuses the production connector, the existing fixture, the existing real-Chrome lock/profile guard/leak-check helpers, and a shared headless wrapper extracted from `capture_real.rs`. Two child stories separate the harness+schema+deterministic surface (CI-runnable without Chrome) from the real-platform evidence capture (opt-in per lane). Chosen because it keeps the smoke cohesive and reviewable, does not touch the product CLI, does not modify production code, and gives each CI lane a clear evidence surface.

### Trickiest unit

The **high-DPI scaling assertion** is the unit with the most novel risk. Chrome’s reported `deviceScaleFactor` under `--force-device-scale-factor=2` is observed but not guaranteed to be exactly `2.0` across Chrome versions/platforms, and the captured JPEG dimensions scale with physical pixels. The assertion must be tolerant enough to pass on real Chrome (≥ 1.5, distinct from the default-DPI run, coherent with `metadata.image()`) while strict enough to catch a regression where the wrapper flag is silently dropped. The deterministic no-Chrome test therefore asserts the wrapper script *contains* the flag, and the real-run test asserts the observed scale is in the high-DPI band and measurably distinct from the default-DPI run recorded in the same evidence pass.

## Implementation units

### Unit 1: Shared headless wrapper and smoke-evidence schema

**Story:** `epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-harness`

**Files:**
- `crates/krometrail-cdp/tests/support/chrome.rs` (extend)
- `crates/krometrail-cdp/tests/capture_real.rs` (behavior-preserving import swap only)
- `crates/krometrail-cdp/tests/support/smoke_evidence.rs` (new)
- `crates/krometrail-cdp/tests/support/mod.rs` (export the new module)
- `docs/evidence/cross-platform-smoke/v1/schema.json` (new)
- `docs/evidence/cross-platform-smoke/v1/README.md` (new)
- `docs/evidence/cross-platform-smoke/v1/sample.json` (new — a schema-valid canonical example, committed for the deterministic round-trip test)

**Shared headless wrapper** — move the private `ChromeWrapper` from `capture_real.rs` into `tests/support/chrome.rs` as a shared, behavior-preserving helper and add the high-DPI variant:

```rust
// crates/krometrail-cdp/tests/support/chrome.rs

/// Test-only Chrome launcher wrapper that forces the headless flags CI runners require and
/// optionally forces a high device-scale-factor for the high-DPI smoke configuration. This is
/// test infrastructure; the production launcher never hardcodes headless/gpu/sandbox.
pub enum ChromeWrapperVariant {
    Headless,
    HighDpi,
}

pub struct ChromeWrapper {
    pub path: std::path::PathBuf,
    pub variant: ChromeWrapperVariant,
}

impl ChromeWrapper {
    /// Writes a unique `exec` shell wrapper for the discovered Chrome/Chromium and returns its
    /// path. `Headless` emits `--headless=new --disable-gpu --no-sandbox "$@"`; `HighDpi` adds
    /// `--high-dpi-support=1 --force-device-scale-factor=2`. The wrapper forwards `"$@"` so the
    /// production launcher’s own arguments still apply.
    pub fn new(variant: ChromeWrapperVariant) -> Option<Self>;
}

impl Drop for ChromeWrapper {
    fn drop(&mut self); // removes the wrapper script, unchanged from capture_real.rs
}
```

`capture_real.rs` replaces its private `ChromeWrapper` with `support::chrome::ChromeWrapper::new(ChromeWrapperVariant::Headless)`. This is a behavior-preserving, test-only move; every acceptance criterion of the done real-Chrome fidelity story remains satisfied because the produced wrapper script is byte-identical for the `Headless` variant.

**Smoke-evidence schema** — `docs/evidence/cross-platform-smoke/v1/schema.json` defines `CrossPlatformSmokeEvidence` (Draft 2020-12). It is intentionally not `$ref`-compatible with `TransportEvidenceV2`. Required top-level fields:

```jsonc
{
  "schema_version": 1,
  "kind": "cross_platform_capture_smoke",
  "provenance": {
    "krometrail_revision": "<git sha>",
    "rust_version": "<toolchain>",
    "cdpkit_version": "0.4.0",
    "platform": "linux" | "macos",
    "architecture": "x86_64" | "aarch64",
    "configuration_name": "linux-chrome" | "linux-chromium"
                            | "macos-chrome-default-dpi" | "macos-chrome-high-dpi",
    "browser_installation": {
      "executable_source": "explicit_request|environment_override|platform_default|path_lookup",
      "product": "chrome|chromium|electron_renderer|other_chromium",
      "discovered_version": "<version>"
    },
    "runtime_version": {
      "product": "...", "product_version": "...", "revision": "...",
      "protocol_version": "...", "user_agent": "...", "js_version": "..."
    },
    "launch": {
      "ownership": "managed",                   // smoke is always managed
      "profile_kind": "temporary",
      "endpoint": "loopback",
      "wrapper_variant": "headless" | "high_dpi",
      "force_device_scale_factor": 2.0 | null
    },
    "capture_config": {
      "queue_capacity": 16,                       // fidelity session
      "loss_queue_capacity": 1,                   // loss-reporting session
      "shutdown_timeout_ms": 12000,
      "max_active_streams": 4
    },
    "fixture": {
      "name": "cdp-transport-gate",
      "path": "tests/fixtures/browser/cdp-transport-gate",
      "index_html_sha256": "<digest>",
      "animation_js_sha256": "<digest>"
    }
  },
  "sessions": [
    {
      "name": "fidelity" | "loss_reporting",
      "frame_count": 30,
      "source_time_samples": 30,
      "image_dimensions": { "width": 780, "height": 437 },
      "viewport": { "width": 780, "height": 437 },
      "device_scale_factor": 1.0,
      "capture_ordinal_range": { "min": 1, "max": 30 },
      "observed_clock_span_nanos": 1234567890,
      "session_clock_span_nanos": 1234567890,
      "ack_latency_nanos":  { "samples": 30, "p50": 0, "p95": 0, "p99": 0, "max": 0 },
      "frame_cadence_nanos": { "samples": 29, "p50": 0, "p95": 0, "p99": 0, "max": 0 },
      "declared_gaps": [ { "reason": "ingestion_queue_saturated", "count": 1 } ],
      "visibility_events": 0
    }
  ],
  "shutdown": {
    "outcome": "managed_browser_closed",
    "flush_count": 1,
    "process_references_after": [],
    "profile_references_after": []
  },
  "non_claims": [
    "no transport requalification (final5 owns cdpkit selection)",
    "no host-speed percentile threshold (ack/cadence are diagnostics)",
    "no product-thesis capture-probability threshold",
    "no duration sweep, defect corpus, artifact comparison, or storage validation",
    "no chrome-acknowledgement-token continuity claim"
  ]
}
```

The Rust mirror in `tests/support/smoke_evidence.rs` is a `serde` struct that serializes to this shape, validates the same invariants the schema encodes (e.g. `outcome ∈ {managed_browser_closed}`, `wrapper_variant ∈ {headless, high_dpi}`, percentile fields null when `samples == 0` and non-null otherwise), and round-trips through `serde_json`. Percentiles follow the existing `CaptureTimingSummary` rule: fixed-bucket upper bounds where exact `max` may fall below `p99`.

The committed `sample.json` is a hand-authored, schema-valid example used by the deterministic round-trip test so the schema is exercised without real Chrome.

**Acceptance criteria:**

- [ ] `ChromeWrapper` exists in `tests/support/chrome.rs` with `Headless` and `HighDpi` variants; `capture_real.rs` uses the shared `Headless` variant with no behavioral change to its wrapper script; the existing `capture_real.rs` opt-in suite still passes 5/5 when run with `KROMETRAIL_REAL_CHROME_TESTS=1`.
- [ ] The `HighDpi` wrapper script contains `--high-dpi-support=1` and `--force-device-scale-factor=2` (asserted by a deterministic test that reads the wrapper bytes; no Chrome needed).
- [ ] `docs/evidence/cross-platform-smoke/v1/schema.json` validates the committed `sample.json` and every `CrossPlatformSmokeEvidence` produced by the serializer (deterministic round-trip test; `additionalProperties: false`).
- [ ] The sanitizer guarantees no evidence field contains a host filesystem path outside the committed fixture path constant, an endpoint URL, a frame payload, a profile path, or a raw adapter error string (deterministic property test over the serializer outputs).
- [ ] `kind`, `schema_version`, `provenance.configuration_name`, `provenance.platform`, `provenance.cdpkit_version`, `shutdown.outcome`, and `non_claims` are required and non-empty.
- [ ] No production code, no `src/cli.rs` change, no new fixture, and no final5 file is modified.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass; `capture_real.rs` opt-in suite remains green when Chrome is available.

### Unit 2: Real-browser smoke and per-configuration evidence capture

**Story:** `epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-real-evidence`

**Depends on:** `epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-harness`

**File:** `crates/krometrail-cdp/tests/cross_platform_smoke.rs` (new)

This file owns the opt-in real-Chrome smoke and the deterministic per-platform skip/leak-helper tests. It reuses the existing fixture bytes, the `FixtureServer` pattern (or `file://` fixture URL via `support::chrome::fixture_url()`), the real-browser lock, the temporary profile guard, the `assert_profile_unreferenced` leak check, the `TestSink`/`TestClock`/`TestIds` ports, and the existing `assert_frame_fidelity` / `assert_strict_ordinals_by_target` helpers (imported from a small `tests/support/smoke_assertions.rs` extracted from `capture_real.rs`, or duplicated minimally — see Simplification).

```rust
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(45);
const STOP_TIMEOUT: Duration = Duration::from_secs(12);
const MIN_FIDELITY_FRAMES: usize = 30;

#[tokio::test]
async fn opt_in_cross_platform_smoke_records_fidelity_loss_and_cleanup_per_configuration() {
    if !real_chrome_test_available() { return; }
    let _browser_lock = support::chrome::real_browser_lock().await;
    for configuration in configurations_for_this_platform() {
        run_fidelity_session(&configuration).await;       // identity + frames + clocks + ordinals + scaling
        run_loss_reporting_session(&configuration).await; // IngestionQueueSaturated honest + drain
        write_evidence(&configuration).await;             // <config>.json to KROMETRAIL_SMOKE_EVIDENCE_DIR
    }
}
```

`configurations_for_this_platform()` returns the `cfg`-gated set:

- `target_os = "linux"` → `[LinuxChrome, LinuxChromiumIfInstalled]`
- `target_os = "macos"` → `[MacosChromeDefaultDpi, MacosChromeHighDpi]`
- other → the test skips with a printed `wrong_platform` reason

Each `Configuration` carries: name, `ChromeWrapperVariant`, expected scale band (`Default` ⇒ `≤ 1.5`, `High` ⇒ `≥ 1.5`), and the discovered installation selector (Chrome vs Chromium).

**Fidelity session assertions (per configuration):**

- `session.ownership() == Managed` and `session.state() == Ready` after initial visibility resolves.
- Runtime identity from `session.compatibility().version()`: `product()`, `product_version()`, `revision()`, `protocol_version()`, `user_agent()`, `js_version()` are all non-empty; the discovered installation’s `product` class matches the runtime `product` class (Chrome/Chromium).
- At least `MIN_FIDELITY_FRAMES` non-empty JPEG frames reach the sink under `CAPTURE_TIMEOUT`.
- Reuse `assert_frame_fidelity` and `assert_strict_ordinals_by_target` unchanged: unique `FrameId`, expected `SessionId`/`TargetId`, JPEG-format, valid JPEG headers matching `metadata.image()`, positive viewport, finite positive `device_scale_factor()`, `session_time ≤ observed_time`, nondecreasing observed/session clocks, `observed_time ≥ session_origin`, non-negative Chrome source time when present.
- **Scaling assertion** (high-DPI configuration only, plus a recorded default-DPI scale in the same evidence pass for distinctness): `metadata.device_scale_factor() ≥ 1.5` on the high-DPI run and the high-DPI scale is strictly greater than the default-DPI scale recorded for the same platform; default-DPI run records scale `≤ 1.5`. Coherence: JPEG dimensions still match `metadata.image()` and viewport remains positive.
- `TargetCaptureStatus` after the session: `ack_latency().sample_count() ≥ MIN_FIDELITY_FRAMES`, `frame_cadence().sample_count() > 0`, and the percentile-ordering invariant `p50 ≤ p95 ≤ p99` holds for both histograms (consistency only; no absolute threshold). The exact `max` may fall below `p99` because percentiles are fixed-bucket upper bounds.
- Every declared gap (usually zero on this fixture) is target-owned with a real `CaptureGapReason`; no inferred silence gap is fabricated from quiet cadence, the ack token, or ordinal arithmetic.
- Managed `stop()` returns `ManagedBrowserClosed` within `STOP_TIMEOUT`; `sink.flush_count() == 1`; `assert_profile_unreferenced(profile_root)` passes; record `process_references_after` and `profile_references_after` (both empty) in evidence.

**Loss-reporting session assertions (per configuration):**

- `CaptureConfig` with `queue_capacity: 1` and a blocked `TestSink`; wait for `clock.calls()` to advance and assert an `IngestionQueueSaturated` gap arrives for the target with `estimated_missing_frames()` `Some(> 0)`.
- Status counters satisfy `received ≥ acknowledged`, `acknowledged == accepted + dropped`, `dropped > 0`, `queue_depth ≤ queue_capacity`, and `ack_latency().sample_count() == received_frames()` — acknowledgement continues ahead of bounded handoff. No host-speed percentile is asserted.
- Release the sink; accepted work drains before a bounded managed `stop()` that returns `ManagedBrowserClosed` with `flush_count() == 1`; leak check passes.
- The incomplete-stop and proxy-sever reconnect scenarios are **not** re-run here; they remain owned by `capture_real.rs`.

**Evidence output:** each configuration writes `<configuration_name>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default `std::env::temp_dir().join(format!("krometrail-smoke-{}-{}", config, pid))`). The test asserts each emitted document validates against `schema.json` and that `provenance.configuration_name`, `provenance.platform`, and the per-session measurements round-trip through `CrossPlatformSmokeEvidence`. The committed evidence lives under `docs/evidence/cross-platform-smoke/v1/` after the operator runs the smoke with the dir pointed there.

**Deterministic no-Chrome tests in the same file:**

- `configurations_for_this_platform()` returns the expected `cfg`-gated set and skips with a reason on unsupported platforms.
- `real_chrome_test_available()` prints the exact skip reason when `KROMETRAIL_REAL_CHROME_TESTS != 1` or `discover_installations(None)` is empty.
- The leak-check helper rejects a known referenced profile and accepts an unreferenced one (reuses `support::chrome::process_references`).
- The evidence serializer rejects an empty `non_claims`, a missing `runtime_version` field, and a `wrapper_variant: "high_dpi"` record whose `force_device_scale_factor` is `null`.

**Acceptance criteria:**

- [ ] On a Linux lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs the `linux-chrome` fidelity + loss-reporting sessions and writes a schema-valid `linux-chrome.json`; if Chromium is also installed, `linux-chromium.json` is produced, otherwise the run records `not_installed` and continues.
- [ ] On a macOS lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs `macos-chrome-default-dpi` and `macos-chrome-high-dpi`, writes both schema-valid JSON documents, and the high-DPI document records `device_scale_factor ≥ 1.5` strictly greater than the default-DPI document’s scale.
- [ ] Every fidelity session produces ≥ 30 non-empty JPEG frames with unique `FrameId`, strict per-target `CaptureOrdinal`, valid JPEG dimensions matching `metadata.image()`, positive viewport, finite positive scale, three clocks preserved (`session_time ≤ observed_time`, nondecreasing, `observed ≥ session_origin`), and runtime identity whose product class matches the discovered installation.
- [ ] Every loss-reporting session declares a target-owned `IngestionQueueSaturated` gap with a positive missing estimate, satisfies the `received/acknowledged/accepted/dropped` counter invariants with `dropped > 0`, drains after release, and ends in a bounded `ManagedBrowserClosed` stop with one flush.
- [ ] No ack/cadence percentile threshold is asserted; histograms are recorded as diagnostics with the percentile-ordering and sample-count invariants only.
- [ ] Every run ends with `process_references_after` and `profile_references_after` both empty; no managed Chrome process or profile reference outlives its session.
- [ ] The committed `docs/evidence/cross-platform-smoke/v1/{linux-chrome,macos-chrome-default-dpi,macos-chrome-high-dpi}.json` documents are schema-valid and carry the recorded measurements; `linux-chromium.json` is committed when Chromium evidence is available and otherwise noted as absent in the README.
- [ ] The deterministic no-Chrome tests pass on every lane; the opt-in real-Chrome tests skip cleanly (printing the reason) when Chrome or the gate is unavailable.
- [ ] No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified by this story.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Implementation order and dependencies

1. `...-smoke-harness` — extract the shared wrapper, land the schema + serializer + sample, and prove the deterministic surface. No real Chrome required; CI-green on every lane.
2. `...-smoke-real-evidence` — depends on the harness; add the real-browser smoke and deterministic skip/leak tests, run it on each lane, and commit the four evidence documents.

The chain is serialized because the real-evidence story imports the harness’s serializer, schema path, and shared wrapper. Splitting them keeps the CI-runnable-without-Chrome surface reviewable on its own and gives each platform lane a single evidence story to execute.

## Simplification and elimination

- **Reuse, do not reinvent.** The smoke reuses the production connector, the existing `cdp-transport-gate` fixture (no new fixture), the existing real-browser lock/profile guard/leak-check helpers, and the existing `TestSink`/`TestClock`/`TestIds` ports. No new production abstraction is introduced.
- **Extract `ChromeWrapper` rather than duplicate it.** Two test files needing a headless wrapper earn one shared home in `tests/support/chrome.rs`. This is a behavior-preserving test-only move folded into the harness story, not a separate `[refactor]` story — its review surface is entirely within the smoke.
- **Prefer minimal assertion helper sharing.** `assert_frame_fidelity` and `assert_strict_ordinals_by_target` are extracted to `tests/support/smoke_assertions.rs` only if both test files use them unchanged; if the smoke’s per-target needs diverge, the smoke keeps its own scoped copy and the extraction is skipped. Decision deferred to the implementor on the smallest non-duplicative choice.
- **No compatibility shim, no fake-success sink, no weakening of `capture_real.rs` acceptance.** The smoke adds evidence; it does not relax existing contracts.
- **No new `src/cli.rs` command, no production code change.** Evidence gathering stays in the test surface.

## Testing

### Deterministic (every CI lane, no Chrome)

- Schema round-trip: `sample.json` and every serializer output validate against `schema.json` with `additionalProperties: false`.
- Sanitizer property test: no host path outside the fixture constant, no endpoint, no payload, no raw adapter error leaks into evidence.
- Wrapper variant test: the `HighDpi` wrapper script bytes contain both high-DPI flags; the `Headless` variant matches the existing `capture_real.rs` script.
- Configuration/skip test: `cfg`-gated set is correct per platform; skip reasons are exact.
- Leak-helper test: referenced profile rejected, unreferenced profile accepted.

### Opt-in real-Chrome (Linux lane and macOS lane)

- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture` runs the fidelity + loss-reporting sessions per configuration and writes the evidence JSON.
- The same command on `capture_real.rs` remains 5/5 green (regression guard for the wrapper extraction).

### Tests deliberately not added

- No duration-sweep, capture-probability, or host-speed percentile test (evaluation epic / final5).
- No per-platform re-run of saturation/incomplete-stop/reconnect (owned by `capture_real.rs`).
- No visual-defect, artifact, or storage assertion (owned downstream).
- No test that Chrome’s acknowledgement token changes or detects skipped frames.

## Risks and pre-mortem

- **Riskiest assumption — `--force-device-scale-factor=2` yields a stable, observable `deviceScaleFactor ≥ 1.5` on macOS Chrome.** If a future Chrome version caps or ignores the flag under headless, the high-DPI assertion fails. Mitigation: the assertion uses a `≥ 1.5` band and a *distinct-from-default-DPI* comparison recorded in the same evidence pass, not an exact `2.0`; the deterministic test guards the flag’s presence in the wrapper; and the `non_claims` list records that the smoke measures observed scale, not perceptual high-DPI correctness.
- **Loss-reporting may not saturate within the deadline on a fast host.** Mitigation: the capacity-one queue and blocked sink match the proven `capture_real.rs` pattern; if saturation does not occur within `CAPTURE_TIMEOUT`, the test records that as an honest skip for the loss-reporting session rather than failing the fidelity session.
- **Linux Chromium absent on the runner.** Mitigation: discovery is best-effort and `not_installed` is an honest skip, not a failure; the README records which configurations were available per release.
- **Wrapper extraction could regress `capture_real.rs`.** Mitigation: the `Headless` variant produces a byte-identical script, the harness story re-runs the `capture_real.rs` opt-in suite (5/5) as a regression guard, and no `capture_real.rs` assertion text changes.
- **Evidence schema drift between Rust struct and JSON schema.** Mitigation: the deterministic round-trip test validates both directions, the schema forbids additional properties, and the canonical `sample.json` is committed alongside the schema.
- **CI lane ambiguity (which platform runs which config).** Mitigation: configurations are `cfg`-gated; a Linux lane cannot run the macOS high-DPI config and vice versa; the wrong-platform case skips with a printed reason.
- **Least certain — whether real Chrome on the macOS lane emits any visibility transition.** The smoke records the count and infers no silence gap; if a transition appears, the existing conditional visibility assertion applies. This matches the limitation `capture_real.rs` already records honestly.
