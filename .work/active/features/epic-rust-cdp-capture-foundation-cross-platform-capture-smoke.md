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

> **Design repair pass (2026-07-13).** This revision applies a complementary review's findings to the
> design committed at `d22b3be` and returns the feature to `implementing`. No production code, no
> subagents, no implementation work, no push. The repair is design-only: it corrects the shared
> `ChromeWrapper` signature (explicit executable path + variant/product selection), forces scale on
> **both** DPI variants, pins one explicit `CaptureConfig` matching production defaults, corrects the
> `capture_real.rs` opt-in count by enumeration, adds real macOS process/profile reference checking,
> defines deterministic canonical evidence bytes, verifies the runtime `BrowserVersion` source, adds a
> `cdpkit-transport` feature boundary, and documents the exact manual evidence procedure plus the
> parent-approval gate. Important findings and nits are addressed below where the change lives.

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

- **Driver:** direct redesign under active autopilot `--all` with no user questions; no subagents were spawned. This repair pass likewise ran inline with no subagents and no implementation.
- **Effective review weight:** standard. The caller prohibited subdelegation, so no design-time independent advisory ran; this does not block design completion. Feature review remains required after implementation.
- **Dispatch rationale:** direct reads covered the feature brief, parent epic and its decomposition risk note, all five foundation docs, the canonical final5 evidence layout (`docs/evidence/cdp-transport/v2/`), the final5 schema (`schema.json`), the rust-CDP transport skill, the done `bounded-screencast-ingestion` feature and its real-Chrome fidelity story, the done `chrome-target-supervision` feature, the existing `crates/krometrail-cdp/tests/capture_real.rs` harness and its `tests/support/chrome.rs` helpers, the production launcher (`launcher/startup.rs`) and its deliberate no-hardcoded-headless/gpu stance, the `krometrail-core` capture/browser accessor surface, and (for this repair) the actual `CaptureConfig::default()` values, the enumerated `capture_real.rs` test count, the Linux-only `process_command_references` implementation, and the runtime `BrowserVersion`/`BrowserCompatibility` accessor surface.
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
- **No source-attestation sha256 tree.** final5 needed a source-file digest tree because it was the *selection* evidence for an external dependency and had to be independently reproducible from source. The smoke's authority is “the production path on this Chrome version on this platform produced these observed measurements,” not a re-derivation from source. Provenance records git revision, Rust toolchain, cdpkit version, discovered installation, runtime `BrowserVersion`, fixture digests, and launch config — proportionate, not maximalist.
- **Evidence is written by the test, committed by an operator.** The real-Chrome smoke writes `<config>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default: a unique temp path). The real-evidence story runs the smoke with that dir pointed at `docs/evidence/cross-platform-smoke/v1/` and commits the JSON documents. The test never mutates the source tree during `cargo test`, mirroring how final5 evidence is gathered out-of-band and then committed. The exact operator commands and the decisive/skip policy are recorded in [Manual evidence procedure and decisive/skip policy](#manual-evidence-procedure-and-decisiveskip-policy) below.
- **High-DPI is macOS-scoped per the brief.** `current stable Chrome on macOS, including a high-DPI configuration` places the high-DPI configuration on macOS. The harness `ChromeWrapperVariant::HighDpi` variant works on Linux too, but the smoke does not run a Linux high-DPI configuration because the brief does not ask for one. This keeps the configuration count at four.
- **Linux Chromium is filtered explicitly, not "first discovered".** The brief says “Chrome or Chromium” on Linux. The smoke discovers both via `krometrail_cdp::discover_installations(None)` and selects by `BrowserInstallation::product`: the `linux-chrome` configuration uses the first installation whose `product == BrowserProduct::Chrome`, the `linux-chromium` configuration uses the first whose `product == BrowserProduct::Chromium`. The wrapper is then constructed from that explicit executable, so the two Linux configurations cannot accidentally share or swap installations. When Chromium is absent, the run records `not_installed` as an honest skip — never a failure.
- **Both DPI variants force scale.** `ChromeWrapperVariant::DefaultDpi` emits `--force-device-scale-factor=1`; `ChromeWrapperVariant::HighDpi` emits `--high-dpi-support=1 --force-device-scale-factor=2`. Forcing the default variant to scale 1 anchors the default-DPI observation to a host-independent baseline (a headless CI runner with no display reports ~1, but a hosted macOS lane with a retina display could otherwise pick a higher default), so the default band assertion is deterministic rather than host-native. The observed bands are asserted, not assumed: default `≤ 1.5`, high `≥ 1.5`, and the high-DPI scale is strictly greater than the default-DPI scale recorded in the same evidence pass.
- **Always-headless via the shared wrapper.** CI runners have no display. The smoke launches every configuration through `ChromeWrapper` (headless), exactly as `capture_real.rs` does, so the smoke is CI-runnable on both lanes without `xvfb` or a macOS display. The production launcher remains unchanged and still does not hardcode headless/gpu/sandbox (its existing test guards this).
- **One explicit CaptureConfig, and it is the production default.** The fidelity session uses `CaptureConfig::default()` verbatim — no field overrides — so the schema records the *actual* production values, not invented ones: `format: jpeg`, `jpeg_quality: 80`, `max_dimensions: null`, `max_active_streams: 8`, `queue_capacity: 4`, `max_base64_payload_bytes: 8_388_608`, `gap_ledger_capacity: 64`, `ack_timeout_ms: 250`, `shutdown_timeout_ms: 5000`. The loss-reporting session is the one and only justified override — `queue_capacity: 1` — because a capacity-one queue is required to declare `IngestionQueueSaturated` honestly; every other field stays at the default. The schema records each session's config snapshot so tests and evidence match the real `CaptureConfig` byte-for-byte. (The prior draft's `queue_capacity: 16 / shutdown_timeout_ms: 12000 / max_active_streams: 4` were fabricated and did not correspond to any real `CaptureConfig`; this repair removes them.)
- **Cross-platform process/profile reference checking.** `tests/support/chrome.rs::process_command_references` is currently `#[cfg(target_os = "linux")]` only — it scans `/proc/*/cmdline` — and returns an empty `Vec` on every other platform (including macOS), so `assert_profile_unreferenced` is silently a no-op on the macOS lane. This repair adds a `#[cfg(target_os = "macos")]` branch that shells out to `ps -ax -o pid= -o command=` and filters by the test-root path, giving macOS a real (not no-op) reference scan with parity to Linux. Both branches return `Vec<String>` of `"pid {pid}: {command}"` so the evidence `process_references_after` / `profile_references_after` fields are populated honestly on both lanes. Until a proven scan lands, the macOS evidence must scope the limitation explicitly rather than record an empty list as if it were verified clean.
- **Deterministic canonical evidence bytes.** Evidence is serialized through `serde` structs whose fields are declared in schema order (serde preserves declaration order), `BTreeMap<String, _>` for any string-keyed map (sorted iteration), and a fixed canonical session order (`fidelity` before `loss_reporting`). `serde_json` is consumed without the `preserve_order` feature, so any `Value::Object` already iterates sorted keys; the serializer additionally applies a recursive key-sort pass (the same shape `krometrail-cdp`'s `spike::contract::canonicalize_value` uses, re-implemented locally so the smoke does not depend on the non-default `cdp-spike` feature) and emits via `serde_json::to_vec_pretty`. The committed `sample.json` is hand-authored to that exact byte layout, so the round-trip test compares canonical bytes, not pretty-printed approximations. This is what makes the committed evidence reproducible across hosts and reviewable in a diff.
- **Runtime `BrowserVersion` source is verified.** The runtime identity accessor path is `session.compatibility().version` (a public *field* on `BrowserCompatibility`, not a `version()` method) yielding a `BrowserVersion`; from there `.product() -> BrowserProduct`, `.product_version() -> &BrowserProductVersion` (read its string with `.as_str()`), `.revision() -> &str`, `.protocol_version() -> &str`, `.user_agent() -> &str`, `.js_version() -> &str`. The discovered-installation product class comes from `BrowserInstallation::product` (same `BrowserProduct` enum), so the “discovered product class matches runtime product class” check compares two values of the same enum. The prior draft's `session.compatibility().version()` was a method call that does not exist; this repair corrects it to field access.
- **`cdpkit-transport` feature boundary.** The smoke file is gated `#![cfg(feature = "cdpkit-transport")]`, matching every other cdpkit-dependent test in the crate (`capture_real.rs`, `chrome_session_real.rs`, `compatibility_probe.rs`, `production_transport.rs`, `session_capture.rs`, `session_supervision.rs`). It is **not** gated on `cdp-spike`. A boundary acceptance check verifies `cargo test -p krometrail-cdp --no-default-features --tests` excludes the smoke (and `capture_real.rs`) from compilation, so the default-off and no-default build configurations stay clean.
- **No visibility forcing per platform.** Real Chrome on these runners emits no `Page.screencastVisibilityChanged` transition (the same honest limitation `capture_real.rs` records). The smoke records the visibility-event count per run and infers no hidden-silence gap; if Chrome does emit a transition, the smoke requires a target-owned `TargetHidden` gap and same-target recovery, reusing the existing conditional visibility assertion.
- **Loss-reporting session is one blocked-sink run, not the full gauntlet.** Per-configuration loss confidence is “`IngestionQueueSaturated` is declared honestly with a positive missing estimate and target-owned identity, and releasing the sink drains accepted work before a bounded managed stop.” That is the smallest honest loss-reporting proof; the incomplete-stop and proxy-sever reconnect proofs stay in `capture_real.rs`.

## Architectural choice

### Option 1: Extend `capture_real.rs` with platform-tagged cases

Add `#[cfg(target_os = …)]` cases and a high-DPI variant directly to the existing real-Chrome test file. Smallest diff. Rejected: `capture_real.rs` is owned by a done story and its four scenarios are transport/ingestion contracts, not platform evidence. Mixing the platform smoke into it blurs ownership, makes the done story’s file the de-facto growth point for a different kind of evidence, and couples the smoke to a file the real-Chrome fidelity story explicitly owns.

### Option 2: A new production command that captures and writes evidence

Ship a `krometrail capture-smoke` subcommand that runs the configurations and writes evidence JSON. Rejected: there is no product command surface for capture yet (`src/cli.rs` exposes only `--version`, `--help`, and `doctor`). Inventing a command to host test evidence violates the project rule against adding command examples for capabilities not present in `src/cli.rs` and pulls evidence gathering into the product runtime.

### Option 3 (chosen): Test-only smoke harness + distinct evidence schema in `krometrail-cdp`, two child stories

A new opt-in test file `crates/krometrail-cdp/tests/cross_platform_smoke.rs` plus a small `tests/support/smoke_evidence.rs` serializer and a committed `docs/evidence/cross-platform-smoke/v1/` schema/README. The harness reuses the production connector, the existing fixture, the existing real-Chrome lock/profile guard/leak-check helpers, and a shared headless wrapper extracted from `capture_real.rs`. Two child stories separate the harness+schema+deterministic surface (CI-runnable without Chrome) from the real-platform evidence capture (opt-in per lane). Chosen because it keeps the smoke cohesive and reviewable, does not touch the product CLI, does not modify production code, and gives each CI lane a clear evidence surface.

### Trickiest unit

The **high-DPI scaling assertion** is the unit with the most novel risk. Both DPI wrappers now force scale explicitly (`--force-device-scale-factor=1` and `=2`), so the wrapper side is deterministic, but Chrome's *reported* `deviceScaleFactor` under a forced flag is observed, not guaranteed to be exactly `1.0`/`2.0` across Chrome versions and platforms, and the captured JPEG dimensions scale with physical pixels. The assertion must be tolerant enough to pass on real Chrome (default band `≤ 1.5`, high band `≥ 1.5`, high strictly greater than default — coherent with `metadata.image()`) while strict enough to catch a regression where a wrapper flag is silently dropped. The deterministic no-Chrome test therefore asserts the wrapper script *contains* both forced-scale flags for the variant, and the real-run test asserts the observed scale lands in its band and is measurably distinct from the default-DPI run recorded in the same evidence pass — without assuming the host display's native scale (the whole reason both variants force scale).

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

**Shared headless wrapper** — move the private `ChromeWrapper` from `capture_real.rs` into `tests/support/chrome.rs` as a shared, parameterized helper. The redesign takes an **explicit executable path** and a **variant**, and offers a pure `script_bytes` function so no-Chrome tests are deterministic:

```rust
// crates/krometrail-cdp/tests/support/chrome.rs

/// Smoke wrapper flag sets. Both variants force device scale so observations are host-independent;
/// `DefaultDpi` anchors the default band to scale 1, `HighDpi` to scale 2.
pub enum ChromeWrapperVariant {
    DefaultDpi,
    HighDpi,
}

/// Which Chromium-family product to select from discovery. `capture_real.rs` selects `Chrome`
/// (preserving today's behavior of picking a Chrome installation); the smoke selects per config so
/// Linux Chromium is filtered explicitly rather than "first discovered".
pub enum ChromeProduct {
    Chrome,
    Chromium,
}

/// Test-only Chrome launcher wrapper. Production launch is unchanged; this only writes the shell
/// wrapper CI runners require. The wrapper script is a pure function of (executable, variant), so a
/// no-Chrome test can pass a sentinel path and assert exact bytes.
pub struct ChromeWrapper {
    pub path: std::path::PathBuf,
    pub variant: ChromeWrapperVariant,
    pub executable: std::path::PathBuf,
    pub product: ChromeProduct,
}

impl ChromeWrapper {
    /// Select the first discovered installation matching `product`, then write the wrapper.
    /// Returns `None` when no matching installation is discovered (Linux Chromium absent).
    #[cfg(unix)]
    pub fn for_product(product: ChromeProduct, variant: ChromeWrapperVariant) -> Option<Self>;

    /// Construct from an explicit, already-selected executable. The wrapper script is a pure
    /// function of (executable, variant); discovery is the caller's responsibility.
    #[cfg(unix)]
    pub fn new(
        executable: std::path::PathBuf,
        product: ChromeProduct,
        variant: ChromeWrapperVariant,
    ) -> Self;

    /// Pure function: the wrapper script bytes for (executable, variant), without touching the
    /// filesystem. Used by the deterministic no-Chrome byte test.
    pub fn script_bytes(executable: &std::path::Path, variant: ChromeWrapperVariant) -> String;
}

impl Drop for ChromeWrapper {
    fn drop(&mut self); // removes the wrapper script, unchanged from capture_real.rs
}
```

`DefaultDpi` emits `#!/bin/sh\nexec {q} --headless=new --disable-gpu --no-sandbox --force-device-scale-factor=1 "$@"\n`; `HighDpi` emits the same line plus `--high-dpi-support=1 --force-device-scale-factor=2` (replacing the `=1`). `{q}` is the shell-quoted executable. `script_bytes` returns exactly these bytes so the deterministic test does not need Chrome or a writable temp dir.

`capture_real.rs` replaces its private `ChromeWrapper` with `support::chrome::ChromeWrapper::for_product(ChromeProduct::Chrome, ChromeWrapperVariant::DefaultDpi)`. This is a **behavior-compatible**, test-only change but no longer byte-identical: the only script difference vs. the prior private wrapper is the added `--force-device-scale-factor=1` flag. That flag is benign for every `capture_real.rs` assertion (none assert `deviceScaleFactor`), and headless Chrome on a display-less CI runner already reports scale `1`, so forcing it changes nothing observable there. The harness story re-runs the enumerated `capture_real.rs` opt-in suite (4 `#[tokio::test]`s — see [capture_real test count](#capture_real-test-count)) as the regression guard; the done real-Chrome fidelity story's acceptance is preserved because no assertion text or observed value changes on a CI lane. The behavior-preservation evidence is the green re-run, not a byte diff.

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
      "wrapper_variant": "default_dpi" | "high_dpi",
      "force_device_scale_factor": 1.0 | 2.0     // both variants force scale; never null
    },
    "capture_config": {                       // fidelity session = CaptureConfig::default() verbatim
      "format": "jpeg",
      "jpeg_quality": 80,
      "max_dimensions": null,
      "max_active_streams": 8,
      "queue_capacity": 4,
      "max_base64_payload_bytes": 8388608,
      "gap_ledger_capacity": 64,
      "ack_timeout_ms": 250,
      "shutdown_timeout_ms": 5000
    },
    "fixture": {
      "name": "cdp-transport-gate",
      "path": "tests/fixtures/browser/cdp-transport-gate",
      "index_html_sha256": "<digest>",
      "animation_js_sha256": "<digest>"
    }
  },
  "sessions": [                          // canonical order: fidelity first, then loss_reporting
    {
      "name": "fidelity" | "loss_reporting",
      "capture_config": {                     // per-session snapshot; loss_reporting overrides only queue_capacity -> 1
        "queue_capacity": 4 | 1,
        "shutdown_timeout_ms": 5000,
        "max_active_streams": 8
        // ...full fidelity snapshot fields mirror provenance.capture_config; loss session records the queue_capacity=1 override
      },
      "frame_count": 30,
      "source_time_samples": 30,
      "image_dimensions": { "width": 780, "height": 437 },   // observed JPEG dimensions; recorded, not assumed; must equal metadata.image()
      "viewport": { "width": 780, "height": 437 },       // observed logical viewport
      "device_scale_factor": 1.0,                  // observed; default config records ≤1.5, high-dpi records ≥1.5
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

The Rust mirror in `tests/support/smoke_evidence.rs` is a `serde` struct that serializes to this shape, validates the same invariants the schema encodes (e.g. `outcome ∈ {managed_browser_closed}`, `wrapper_variant ∈ {default_dpi, high_dpi}`, `force_device_scale_factor ∈ {1.0, 2.0}`, percentile fields null when `samples == 0` and non-null otherwise), percentile ordering `p50 ≤ p95 ≤ p99`, and `ack_timeout_ms`/`shutdown_timeout_ms` matching the actual `CaptureConfig`), and round-trips through `serde_json`. **Canonical byte order is enforced by the serializer** (see [Design decisions](#design-decisions)): struct fields declared in schema order, `BTreeMap<String, _>` for any string-keyed map, sessions emitted in canonical order (`fidelity` before `loss_reporting`), and a recursive key-sort pass before `serde_json::to_vec_pretty`. Percentiles follow the existing `CaptureTimingSummary` rule: fixed-bucket upper bounds where exact `max` may fall below `p99`. The `capture_config` recorded per session is a faithful snapshot of the `CaptureConfig` value passed to the connector — fidelity records `CaptureConfig::default()` verbatim, loss-reporting records only the `queue_capacity: 1` override — so tests and evidence match the real runtime values.

The committed `sample.json` is a hand-authored, schema-valid example used by the deterministic round-trip test so the schema is exercised without real Chrome.

**Acceptance criteria:**

- [ ] `ChromeWrapper` exists in `tests/support/chrome.rs` parameterized by explicit `executable: PathBuf`, `product: ChromeProduct`, and `variant: ChromeWrapperVariant` (`DefaultDpi`, `HighDpi`); a pure `script_bytes(executable, variant)` helper returns the wrapper bytes without filesystem access; a `for_product(product, variant)` constructor filters `discover_installations(None)` by `BrowserInstallation::product` so Linux Chrome and Linux Chromium are selected explicitly. `capture_real.rs` migrates to `ChromeWrapper::for_product(ChromeProduct::Chrome, ChromeWrapperVariant::DefaultDpi)`; its enumerated opt-in suite (4 `#[tokio::test]`s — see [capture_real test count](#capture_real-test-count)) re-runs green when `KROMETRAIL_REAL_CHROME_TESTS=1`.
- [ ] The `DefaultDpi` wrapper script contains `--headless=new`, `--disable-gpu`, `--no-sandbox`, and `--force-device-scale-factor=1`; the `HighDpi` script additionally contains `--high-dpi-support=1` and `--force-device-scale-factor=2`. Both are asserted by a deterministic test that reads `ChromeWrapper::script_bytes` (no Chrome, no writable temp dir needed).
- [ ] `docs/evidence/cross-platform-smoke/v1/schema.json` validates the committed `sample.json` and every `CrossPlatformSmokeEvidence` produced by the serializer (deterministic round-trip test; `additionalProperties: false`).
- [ ] The canonical-bytes test asserts `serde_json::to_vec_pretty(&CrossPlatformSmokeEvidence::sample())` round-trips to the exact committed `sample.json` bytes: struct fields in schema order, `BTreeMap`-sorted maps, sessions in `fidelity` then `loss_reporting` order, and recursive key sort applied. No host-derived ordering leaks into evidence.
- [ ] The sanitizer guarantees no evidence field contains a host filesystem path outside the committed fixture path constant, an endpoint URL, a frame payload, a profile path, or a raw adapter error string (deterministic property test over the serializer outputs).
- [ ] `kind`, `schema_version`, `provenance.configuration_name`, `provenance.platform`, `provenance.cdpkit_version`, `provenance.capture_config.*` (matching `CaptureConfig::default()` exactly), `provenance.launch.force_device_scale_factor ∈ {1.0, 2.0}`, `shutdown.outcome`, and `non_claims` are required and non-empty.
- [ ] The runtime `BrowserVersion` accessor test confirms the evidence path uses `session.compatibility().version` (field) and reads `.product()`, `.product_version().as_str()`, `.revision()`, `.protocol_version()`, `.user_agent()`, `.js_version()`; the discovered `BrowserInstallation::product` and runtime `BrowserVersion::product()` are the same enum and are recorded consistently.
- [ ] `process_command_references` has a real `#[cfg(target_os = "macos")]` branch using `ps -ax -o pid= -o command=` (parity with the Linux `/proc` scan) so macOS leak checks are not silent no-ops; the deterministic test proves a referenced root is reported on both Linux and macOS builds (macOS path exercised via a `cfg`-gated unit test).
- [ ] No production code, no `src/cli.rs` change, no new fixture, and no final5 file is modified.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass; `capture_real.rs` opt-in suite remains green when Chrome is available.
- [ ] **Feature boundary / no-default:** `crates/krometrail-cdp/tests/cross_platform_smoke.rs` opens with `#![cfg(feature = "cdpkit-transport")]`; `cargo test -p krometrail-cdp --no-default-features --tests --locked` succeeds and does not compile the smoke (verified by a deterministic boundary check that confirms the test target is absent under `--no-default-features`).

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

Each `Configuration` carries: name, `ChromeWrapperVariant` (`DefaultDpi` or `HighDpi`), `ChromeProduct` selector (`Chrome` or `Chromium`) used to filter `discover_installations`, expected scale band (`Default` ⇒ `≤ 1.5` anchored to forced scale 1, `High` ⇒ `≥ 1.5` anchored to forced scale 2), and the canonical output filename.

**Fidelity session assertions (per configuration):**

- `session.ownership() == Managed` and `session.state() == Ready` after initial visibility resolves.
- Runtime identity from `session.compatibility().version` (a public *field*, not a method): `version.product()` (`BrowserProduct`), `version.product_version().as_str()`, `version.revision()`, `version.protocol_version()`, `version.user_agent()`, `version.js_version()` are all non-empty; the discovered `BrowserInstallation::product` matches the runtime `version.product()` (same enum) so a Chrome selector cannot silently capture a Chromium process or vice versa.
- Fidelity session uses `CaptureConfig::default()` verbatim — `queue_capacity: 4`, `max_active_streams: 8`, `shutdown_timeout: 5 s`, `ack_timeout: 250 ms`, `jpeg_quality: 80`, `format: jpeg`, `gap_ledger_capacity: 64`, `max_base64_payload_bytes: 8 MiB`. At least `MIN_FIDELITY_FRAMES` non-empty JPEG frames reach the sink under `CAPTURE_TIMEOUT`.
- Reuse `assert_frame_fidelity` and `assert_strict_ordinals_by_target` unchanged: unique `FrameId`, expected `SessionId`/`TargetId`, JPEG-format, valid JPEG headers matching `metadata.image()`, positive viewport, finite positive `device_scale_factor()`, `session_time ≤ observed_time`, nondecreasing observed/session clocks, `observed_time ≥ session_origin`, non-negative Chrome source time when present.
- **Scaling assertion** (recorded for *both* DPI configurations in the same evidence pass, since both wrappers now force scale): default-DPI run records `metadata.device_scale_factor() ≤ 1.5` (anchored to forced scale 1); high-DPI run records `≥ 1.5` (anchored to forced scale 2); the high-DPI scale is strictly greater than the default-DPI scale. No host-native display assumption is made — the assertion is on the observed band, not on an assumed host default. Coherence: JPEG dimensions still match `metadata.image()` and viewport remains positive.
- `TargetCaptureStatus` after the session: `ack_latency().sample_count() ≥ MIN_FIDELITY_FRAMES`, `frame_cadence().sample_count() > 0`, and the percentile-ordering invariant `p50 ≤ p95 ≤ p99` holds for both histograms (consistency only; no absolute threshold). The exact `max` may fall below `p99` because percentiles are fixed-bucket upper bounds.
- Every declared gap (usually zero on this fixture) is target-owned with a real `CaptureGapReason`; no inferred silence gap is fabricated from quiet cadence, the ack token, or ordinal arithmetic.
- Managed `stop()` returns `ManagedBrowserClosed` within `STOP_TIMEOUT`; `sink.flush_count() == 1`; `assert_profile_unreferenced(profile_root)` passes (real reference scan on both Linux `/proc` and macOS `ps`); record `process_references_after` and `profile_references_after` (both empty) in evidence.

**Loss-reporting session assertions (per configuration):**

- `CaptureConfig` with `queue_capacity: 1` overridden on `CaptureConfig::default()` (the *one* justified override — every other field stays default) and a blocked `TestSink`; wait for `clock.calls()` to advance and assert an `IngestionQueueSaturated` gap arrives for the target with `estimated_missing_frames()` returning `Some(_)` (the `Option<NonZeroU64>` already guarantees a positive count).
- Status counters satisfy `received_frames() ≥ acknowledged_frames()`, `acknowledged_frames() == accepted_frames() + dropped_frames()`, `dropped_frames() > 0`, `queue_depth() ≤ queue_capacity()`, and `ack_latency().sample_count() == received_frames()` — acknowledgement continues ahead of bounded handoff. No host-speed percentile is asserted.
- Release the sink; accepted work drains before a bounded managed `stop()` that returns `ManagedBrowserClosed` with `flush_count() == 1`; leak check passes.
- The incomplete-stop and proxy-sever reconnect scenarios are **not** re-run here; they remain owned by `capture_real.rs`.

**Evidence output:** each configuration writes `<configuration_name>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default `std::env::temp_dir().join(format!("krometrail-smoke-{}-{}", config, pid))`). The test asserts each emitted document validates against `schema.json`, that `provenance.capture_config` equals the actual `CaptureConfig` snapshot (fidelity) or the single-field override (loss), and that `provenance.configuration_name`, `provenance.platform`, and the per-session measurements round-trip through `CrossPlatformSmokeEvidence`. The committed evidence lives under `docs/evidence/cross-platform-smoke/v1/` after the operator runs the smoke with the dir pointed there (see [Manual evidence procedure and decisive/skip policy](#manual-evidence-procedure-and-decisiveskip-policy)).

**Deterministic no-Chrome tests in the same file:**

- `configurations_for_this_platform()` returns the expected `cfg`-gated set and skips with a reason on unsupported platforms.
- `real_chrome_test_available()` prints the exact skip reason when `KROMETRAIL_REAL_CHROME_TESTS != 1` or no installation matches the configuration's `ChromeProduct` selector.
- The product-filter helper returns the Chrome installation for `ChromeProduct::Chrome`, the Chromium installation for `ChromeProduct::Chromium`, and `None` when the requested product is absent (no accidental cross-pollination).
- The leak-check helper rejects a known referenced root and accepts an unreferenced one on both Linux and macOS builds (reuses `support::chrome::process_references`).
- The evidence serializer rejects an empty `non_claims`, a missing `runtime_version` field, a `force_device_scale_factor` that is `null` or outside `{1.0, 2.0}`, and a `wrapper_variant`/`force_device_scale_factor` mismatch (e.g. `default_dpi` with `2.0`).

**Acceptance criteria:**

- [ ] On a Linux lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke selects the `BrowserProduct::Chrome` installation (filtered explicitly, not first-discovered) for `linux-chrome` and runs the fidelity + loss-reporting sessions, writing a schema-valid `linux-chrome.json` whose `provenance.capture_config` equals `CaptureConfig::default()`; if a `BrowserProduct::Chromium` installation is also discovered, `linux-chromium.json` is produced via the same explicit product filter, otherwise the run records `not_installed` and continues.
- [ ] On a macOS lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs `macos-chrome-default-dpi` (`DefaultDpi`, forces scale 1) and `macos-chrome-high-dpi` (`HighDpi`, forces scale 2), writes both schema-valid JSON documents, and the high-DPI document records `device_scale_factor ≥ 1.5` strictly greater than the default-DPI document’s scale (which is `≤ 1.5`).
- [ ] Every fidelity session produces ≥ 30 non-empty JPEG frames with unique `FrameId`, strict per-target `CaptureOrdinal`, valid JPEG dimensions matching `metadata.image()`, positive viewport, finite positive scale, three clocks preserved (`session_time ≤ observed_time`, nondecreasing, `observed ≥ session_origin`), and runtime identity (`session.compatibility().version`) whose `BrowserProduct` matches the discovered `BrowserInstallation::product`.
- [ ] Every loss-reporting session declares a target-owned `IngestionQueueSaturated` gap with `estimated_missing_frames()` returning `Some(_)`, satisfies the `received_frames() / acknowledged_frames() / accepted_frames() / dropped_frames()` counter invariants with `dropped_frames() > 0`, drains after release, and ends in a bounded `ManagedBrowserClosed` stop with one flush.
- [ ] No ack/cadence percentile threshold is asserted; histograms are recorded as diagnostics with the percentile-ordering and sample-count invariants only.
- [ ] Every run ends with `process_references_after` and `profile_references_after` both empty *and verified by a real reference scan* (Linux `/proc/*/cmdline`, macOS `ps -ax -o pid= -o command=`) — not a silent no-op; no managed Chrome process or profile reference outlives its session. macOS evidence that predates the `ps`-based scan must record the limitation in `non_claims` rather than asserting an unverified empty list.
- [ ] The committed `docs/evidence/cross-platform-smoke/v1/{linux-chrome,macos-chrome-default-dpi,macos-chrome-high-dpi}.json` documents are schema-valid, carry the recorded measurements, and whose `capture_config` fields match the actual `CaptureConfig` values used; `linux-chromium.json` is committed when Chromium evidence is available and otherwise noted as absent in the README.
- [ ] The deterministic no-Chrome tests pass on every lane; the opt-in real-Chrome tests skip cleanly (printing the reason) when Chrome or the gate is unavailable.
- [ ] No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified by this story.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## capture_real test count

The regression-guard count is **determined by enumeration, not hardcoded**. `crates/krometrail-cdp/tests/capture_real.rs` currently contains exactly four `#[tokio::test]` functions:

1. `opt_in_real_chrome_capture_records_fidelity_and_managed_cleanup`
2. `opt_in_real_chrome_capture_isolates_two_targets_and_records_visibility_when_available`
3. `opt_in_real_chrome_capture_bounds_saturation_and_reports_incomplete_blocked_stop`
4. `opt_in_real_chrome_capture_fences_one_disconnect_and_resets_generation_identity`

The harness story's regression-guard criterion is therefore phrased as "the enumerated `capture_real.rs` opt-in suite re-runs green" and the enumeration is recorded above so a future addition (a fifth test) updates this list rather than a magic `5/5` constant. The prior draft's `5/5` was wrong (the file has four tests) and brittle (a count constant drifts when tests are added). Any acceptance wording that names a count must cite this enumeration.

## Manual evidence procedure and decisive/skip policy

Evidence is gathered out-of-band by an operator on each lane and committed; the test never mutates the source tree during `cargo test`. The exact commands:

**Linux lane** (Chrome installed; Chromium optional):

```bash
# From a clean checkout at the pinned revision:
KROMETRAIL_REAL_CHROME_TESTS=1 \
KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 \
cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored
```

This produces `docs/evidence/cross-platform-smoke/v1/linux-chrome.json` always and `linux-chromium.json` only when a `BrowserProduct::Chromium` installation is discovered; the run prints `not_installed` and continues otherwise. The operator commits whichever files were produced and notes Chromium's absence in the README when relevant.

**macOS lane** (Chrome installed):

```bash
KROMETRAIL_REAL_CHROME_TESTS=1 \
KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 \
cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored
```

This produces both `macos-chrome-default-dpi.json` and `macos-chrome-high-dpi.json`. Both must be present and schema-valid for the macOS lane to count.

**Decisive vs. skip policy:**

- **Decisive configurations** (must be present and green for parent approval): `linux-chrome`, `macos-chrome-default-dpi`, `macos-chrome-high-dpi`. These three cover the production live-capture path on every supported platform × DPI dimension named in the brief.
- **Best-effort configuration** (honest skip permitted): `linux-chromium`. The brief names “Chrome *or* Chromium,” so Chromium evidence is recorded when available and honestly absent otherwise; it never blocks.
- **Wrong platform:** the test skips with a printed `wrong_platform` reason on any `target_os` outside `{linux, macos}` and is not part of any lane's decisive set.
- **Skip, not fail:** when `KROMETRAIL_REAL_CHROME_TESTS != 1`, or no decisive installation is discovered, the test skips cleanly with the exact reason printed to `--nocapture` output. It never turns an absent environment into a failure.

**Parent-approval gate:** Yes — this foundation gate requires all three decisive configurations to be present and schema-valid before `epic-rust-cdp-capture-foundation` (the parent epic) can advance to done. `linux-chromium` is not on that critical path. If a decisive configuration cannot be produced on its lane (Chrome unavailable, scale band not met, reference scan non-empty), the feature stays `implementing`/`review` and the parent cannot close; the obstruction is recorded in the feature body and the README rather than waived.

## Implementation order and dependencies

1. `...-smoke-harness` — extract the shared wrapper, land the schema + serializer + sample, and prove the deterministic surface. No real Chrome required; CI-green on every lane.
2. `...-smoke-real-evidence` — depends on the harness; add the real-browser smoke and deterministic skip/leak tests, run it on each lane, and commit the four evidence documents.

The chain is serialized because the real-evidence story imports the harness’s serializer, schema path, and shared wrapper. Splitting them keeps the CI-runnable-without-Chrome surface reviewable on its own and gives each platform lane a single evidence story to execute.

## Simplification and elimination

- **Reuse, do not reinvent.** The smoke reuses the production connector, the existing `cdp-transport-gate` fixture (no new fixture), the existing real-browser lock/profile guard/leak-check helpers, and the existing `TestSink`/`TestClock`/`TestIds` ports. No new production abstraction is introduced.
- **Extract `ChromeWrapper` rather than duplicate it.** Two test files needing a headless wrapper earn one shared home in `tests/support/chrome.rs`. The extraction is folded into the harness story, not a separate `[refactor]` story — its review surface is entirely within the smoke. The shared wrapper is **behavior-compatible** with the prior private one (the only script change is the added `--force-device-scale-factor=1` on the default variant, benign for `capture_real.rs`'s assertions); the regression evidence is the green re-run of the enumerated 4-test opt-in suite, not a byte-identical diff.
- **Prefer minimal assertion helper sharing.** `assert_frame_fidelity` and `assert_strict_ordinals_by_target` are extracted to `tests/support/smoke_assertions.rs` only if both test files use them unchanged; if the smoke’s per-target needs diverge, the smoke keeps its own scoped copy and the extraction is skipped. Decision deferred to the implementor on the smallest non-duplicative choice.
- **No compatibility shim, no fake-success sink, no weakening of `capture_real.rs` acceptance.** The smoke adds evidence; it does not relax existing contracts.
- **No new `src/cli.rs` command, no production code change.** Evidence gathering stays in the test surface.

## Testing

### Deterministic (every CI lane, no Chrome)

- Schema round-trip: `sample.json` and every serializer output validate against `schema.json` with `additionalProperties: false`.
- Canonical-bytes test: `serde_json::to_vec_pretty` of the sample struct equals the committed `sample.json` byte-for-byte (ordered structs, `BTreeMap`-sorted maps, canonical session order, recursive key sort).
- Sanitizer property test: no host path outside the fixture constant, no endpoint, no payload, no raw adapter error leaks into evidence.
- Wrapper variant test: `ChromeWrapper::script_bytes` for `DefaultDpi` contains `--force-device-scale-factor=1` and the headless flags; for `HighDpi` it additionally contains `--high-dpi-support=1` and `--force-device-scale-factor=2`. Pure function, no Chrome and no writable temp dir required.
- Product-filter test: `discover_installations(None)` filtered by `BrowserProduct::Chrome` and `BrowserProduct::Chromium` returns the right installation each and `None` when the requested product is absent.
- Configuration/skip test: `cfg`-gated set is correct per platform; skip reasons are exact; `--no-default-features` boundary excludes the smoke target.
- Leak-helper test: referenced root rejected, unreferenced root accepted, on both Linux and macOS builds.
- `BrowserVersion` accessor test: `session.compatibility().version` (field) reads all six sub-accessors non-empty on a scripted session.

### Opt-in real-Chrome (Linux lane and macOS lane)

- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored` runs the fidelity + loss-reporting sessions per configuration and writes the evidence JSON.
- The same gate on `capture_real.rs` remains green for its enumerated four tests (regression guard for the wrapper extraction).

### Tests deliberately not added

- No duration-sweep, capture-probability, or host-speed percentile test (evaluation epic / final5).
- No per-platform re-run of saturation/incomplete-stop/reconnect (owned by `capture_real.rs`).
- No visual-defect, artifact, or storage assertion (owned downstream).
- No test that Chrome’s acknowledgement token changes or detects skipped frames.

## Risks and pre-mortem

- **Riskiest assumption — `--force-device-scale-factor=2` yields a stable, observable `deviceScaleFactor ≥ 1.5` on macOS Chrome (and `=1` yields `≤ 1.5` for the default).** If a future Chrome version caps or ignores the flag under headless, the band assertion fails. Mitigation: *both* variants now force scale, so the default-vs-high comparison is host-independent; the assertion uses a `≤ 1.5`/`≥ 1.5` band and a *distinct-from-default-DPI* comparison recorded in the same evidence pass, not an exact value; the deterministic test guards both flags’ presence in the wrapper; and the `non_claims` list records that the smoke measures observed scale, not perceptual high-DPI correctness.
- **macOS reference scan parity.** The leak check was previously a silent no-op on macOS (Linux-only `/proc` scan). The repair adds a `ps`-based scan, but `ps` invocation or parsing differences across macOS versions could under-report. Mitigation: the deterministic test proves a referenced root is reported on the macOS build before any real-Chrome run; until proven, macOS evidence records the limitation in `non_claims` rather than asserting an unverified empty list.
- **Loss-reporting may not saturate within the deadline on a fast host.** Mitigation: the capacity-one queue and blocked sink match the proven `capture_real.rs` pattern; if saturation does not occur within `CAPTURE_TIMEOUT`, the test records that as an honest skip for the loss-reporting session rather than failing the fidelity session.
- **Linux Chromium absent on the runner.** Mitigation: discovery is best-effort, selected explicitly by `BrowserProduct::Chromium`, and `not_installed` is an honest skip (not on the parent-approval critical path), not a failure; the README records which configurations were available per release.
- **Wrapper extraction could regress `capture_real.rs`.** Mitigation: the shared `DefaultDpi` wrapper differs from the prior private wrapper only by the benign `--force-device-scale-factor=1` flag (no `capture_real.rs` assertion reads scale; CI-lane headless already reports ~1); the harness story re-runs the enumerated `capture_real.rs` opt-in suite (four `#[tokio::test]`s) as a regression guard, and no `capture_real.rs` assertion text changes.
- **Evidence schema drift between Rust struct and JSON schema.** Mitigation: the deterministic round-trip test validates both directions, the schema forbids additional properties, the canonical `sample.json` is committed alongside the schema, and the canonical-bytes test pins the exact serialized output.
- **CaptureConfig drift between schema and runtime.** Mitigation: the fidelity session pins `CaptureConfig::default()` verbatim and the schema’s `capture_config` fields are derived from its actual values; the loss session records only the `queue_capacity: 1` override; the deterministic test asserts the serialized config equals the real `CaptureConfig` snapshot.
- **CI lane ambiguity (which platform runs which config).** Mitigation: configurations are `cfg`-gated; a Linux lane cannot run the macOS high-DPI config and vice versa; the wrong-platform case skips with a printed reason.
- **Least certain — whether real Chrome on the macOS lane emits any visibility transition.** The smoke records the count and infers no silence gap; if a transition appears, the existing conditional visibility assertion applies. This matches the limitation `capture_real.rs` already records honestly.
