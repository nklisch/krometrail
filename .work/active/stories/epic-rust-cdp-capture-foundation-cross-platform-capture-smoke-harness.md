---
id: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-harness
kind: story
stage: implementing
tags: [browser, testing, infra]
parent: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Smoke harness, shared wrapper, and evidence schema

## Scope

Land the test-only scaffolding the cross-platform smoke depends on, with no real-Chrome requirement. This story delivers Unit 1 of the feature design (revised by the 2026-07-13 design-repair pass):

- Extract `capture_real.rs`'s private `ChromeWrapper` into `tests/support/chrome.rs` as a **parameterized** shared helper: explicit `executable: PathBuf`, `product: BrowserProduct`, and `variant: ChromeWrapperVariant` (`DefaultDpi`, `HighDpi`), plus a pure `script_bytes(executable, variant)` and a `for_product(product, variant)` discovery-filtering constructor.
- Force scale on **both** variants — `DefaultDpi` ⇒ `--force-device-scale-factor=1`, `HighDpi` ⇒ `--high-dpi-support=1 --force-device-scale-factor=2` — so default-band observations are host-independent.
- Add the `CrossPlatformSmokeEvidence` schema, Rust serializer, canonical sample, and README under `docs/evidence/cross-platform-smoke/v1/`, with `capture_config` fields matching `CaptureConfig::default()` exactly and `force_device_scale_factor ∈ {1.0, 2.0}`.
- Pin deterministic **canonical evidence bytes**: struct fields in schema order, `BTreeMap`-sorted maps, canonical session order (`fidelity` then `loss_reporting`), recursive key sort, emitted via `serde_json::to_vec_pretty`; the committed `sample.json` matches byte-for-byte.
- Verify the runtime `BrowserVersion` source path (`session.compatibility().version` field → `.product()` / `.product_version().as_str()` / `.revision()` / `.protocol_version()` / `.user_agent()` / `.js_version()`).
- Add a real `#[cfg(target_os = "macos")] fn process_command_references` using `ps -ax -o pid= -o command=` (parity with the Linux `/proc` scan) so macOS leak checks are not silent no-ops.
- Add the deterministic no-Chrome tests that guard the schema, the wrapper bytes for both variants, the product filter, the per-platform configuration/skip logic, the canonical-bytes round trip, the `BrowserVersion` accessors, the macOS reference scan, the leak-check helper, and the `--no-default-features` feature boundary.

This story is CI-green on every lane without Chrome installed.

## Required files

- `crates/krometrail-cdp/tests/support/chrome.rs` (extend — add `ChromeWrapperVariant`, parameterized `ChromeWrapper` using core `BrowserProduct` with `script_bytes` + `for_product`, and the macOS `process_command_references` branch)
- `crates/krometrail-cdp/tests/support/mod.rs` (export new module(s))
- `crates/krometrail-cdp/tests/support/smoke_evidence.rs` (new — serde struct + canonical-bytes serializer + sanitizer + schema path)
- `crates/krometrail-cdp/tests/capture_real.rs` (behavior-compatible import swap to `ChromeWrapper::for_product(BrowserProduct::Chrome, ChromeWrapperVariant::DefaultDpi)`)
- `docs/evidence/cross-platform-smoke/v1/schema.json` (new — Draft 2020-12, `additionalProperties: false`, `force_device_scale_factor ∈ {1.0, 2.0}`)
- `docs/evidence/cross-platform-smoke/v1/sample.json` (new — hand-authored schema-valid canonical example pinned to the canonical-bytes layout)
- `docs/evidence/cross-platform-smoke/v1/README.md` (new — provenance, measurements, honest non-claims, lane/convention notes, exact manual commands, decisive/skip policy)

No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion text is modified.

## Implementation notes

- `ChromeWrapper::script_bytes(executable, variant)` is a pure function: `DefaultDpi` returns `#!/bin/sh\nexec {q(executable)} --headless=new --disable-gpu --no-sandbox --force-device-scale-factor=1 "$@"\n`; `HighDpi` returns the same with `--high-dpi-support=1 --force-device-scale-factor=2` replacing `=1`. The deterministic byte test calls this with a sentinel path — no Chrome, no writable temp dir.
- `ChromeWrapper::for_product(product, variant)` calls `discover_installations(None)`, returns the first installation whose `BrowserInstallation::product` matches the requested `BrowserProduct`, and constructs the wrapper from its explicit `executable`. Returns `None` when the product is absent (Linux Chromium missing).
- `capture_real.rs` migrates to `for_product(BrowserProduct::Chrome, ChromeWrapperVariant::DefaultDpi)`. The only script change vs. the prior private wrapper is the added `--force-device-scale-factor=1` flag — benign (no `capture_real.rs` assertion reads scale; CI headless already reports ~1). The behavior-preservation evidence is the green re-run of the enumerated opt-in suite, not a byte diff.
- The macOS `process_command_references` shells out to `ps -ax -o pid= -o command=`, splits each line on the first whitespace into `(pid, command)`, and keeps lines whose `command` contains the test-root path needle — returning `Vec<String>` of `"pid {pid}: {command}"` (parity with the Linux `/proc/*/cmdline` scan).
- `CrossPlatformSmokeEvidence` serializes exactly to the schema; the serializer enforces the same invariants the schema encodes (outcome/variant enums, `force_device_scale_factor ∈ {1.0, 2.0}` and matching `wrapper_variant`, percentile nullability tied to `samples == 0`, percentile ordering `p50 ≤ p95 ≤ p99`, `capture_config` fields matching the actual `CaptureConfig`, required non-empty `non_claims`).
- Canonical bytes: `serde` struct fields in schema-declaration order; `BTreeMap<String, _>` for any string-keyed map; sessions always emitted in `fidelity` then `loss_reporting` order; a recursive key-sort pass (re-implemented locally, mirroring `krometrail-cdp::spike::contract::canonicalize_value`, so the smoke does not depend on the non-default `cdp-spike` feature) before `serde_json::to_vec_pretty`.
- The sanitizer rejects host paths outside the committed fixture constant, endpoint URLs, frame payloads, profile paths, and raw adapter error strings.
- The README names the four configurations, the `$KROMETRAIL_SMOKE_EVIDENCE_DIR` convention (default unique temp path), the per-platform lane assignment, the exact Linux/macOS evidence commands, the decisive/skip policy, and the explicit list of evidence that exists vs. is honestly absent (e.g. Linux Chromium when not installed).

## Acceptance criteria

- [ ] `ChromeWrapper` exists in `tests/support/chrome.rs` parameterized by explicit `executable: PathBuf`, `product: BrowserProduct`, and `variant: ChromeWrapperVariant`; a pure `script_bytes(executable, variant)` returns wrapper bytes without filesystem access; a `for_product(product, variant)` constructor filters `discover_installations(None)` by `BrowserInstallation::product`. `capture_real.rs` migrates to `for_product(BrowserProduct::Chrome, ChromeWrapperVariant::DefaultDpi)` and its enumerated opt-in suite (four `#[tokio::test]`s — see the feature's "capture_real test count" section) re-runs green when `KROMETRAIL_REAL_CHROME_TESTS=1`.
- [ ] The `DefaultDpi` wrapper script contains `--headless=new`, `--disable-gpu`, `--no-sandbox`, and `--force-device-scale-factor=1`; the `HighDpi` script additionally contains `--high-dpi-support=1` and `--force-device-scale-factor=2` (asserted deterministically via `script_bytes` with a sentinel path — no Chrome, no temp dir).
- [ ] `docs/evidence/cross-platform-smoke/v1/schema.json` validates the committed `sample.json` and every `CrossPlatformSmokeEvidence` produced by the serializer (deterministic round-trip test; `additionalProperties: false`); `provenance.capture_config.*` matches `CaptureConfig::default()` exactly and `force_device_scale_factor ∈ {1.0, 2.0}`.
- [ ] The canonical-bytes test asserts `serde_json::to_vec_pretty(&CrossPlatformSmokeEvidence::sample())` equals the committed `sample.json` byte-for-byte (ordered structs, `BTreeMap`-sorted maps, canonical session order, recursive key sort).
- [ ] The sanitizer guarantees no evidence field contains a host filesystem path outside the committed fixture path constant, an endpoint URL, a frame payload, a profile path, or a raw adapter error string (deterministic property test over the serializer outputs).
- [ ] `kind`, `schema_version`, `provenance.configuration_name`, `provenance.platform`, `provenance.cdpkit_version`, `provenance.capture_config.*`, `provenance.launch.force_device_scale_factor`, `shutdown.outcome`, and `non_claims` are required and non-empty.
- [ ] The runtime `BrowserVersion` accessor test confirms the evidence path uses `session.compatibility().version` (field) and reads `.product()`, `.product_version().as_str()`, `.revision()`, `.protocol_version()`, `.user_agent()`, `.js_version()` on a scripted session; discovered `BrowserInstallation::product` and runtime `BrowserVersion::product()` are the same enum.
- [ ] `process_command_references` has a real `#[cfg(target_os = "macos")]` branch using `ps -ax -o pid= -o command=` (parity with the Linux `/proc` scan); the deterministic test proves a referenced root is reported and an unreferenced root is not, on both Linux and macOS builds.
- [ ] No production code, no `src/cli.rs` change, no new fixture, and no final5 file is modified.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass; `capture_real.rs` opt-in suite remains green when Chrome is available.
- [ ] **Feature boundary / no-default:** `crates/krometrail-cdp/tests/cross_platform_smoke.rs` opens with `#![cfg(feature = "cdpkit-transport")]`; `cargo test -p krometrail-cdp --no-default-features --tests --locked` succeeds and does not compile the smoke (verified by a deterministic boundary check confirming the test target is absent under `--no-default-features`).

## Execution

- Effective worker: highest.
- No depends_on; this is the root of the smoke subtree.
