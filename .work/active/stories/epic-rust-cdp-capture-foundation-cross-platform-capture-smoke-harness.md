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

Land the test-only scaffolding the cross-platform smoke depends on, with no real-Chrome requirement. This story delivers Unit 1 of the feature design:

- Extract `capture_real.rs`'s private `ChromeWrapper` into `tests/support/chrome.rs` as a shared helper with `Headless` and `HighDpi` variants.
- Add the `CrossPlatformSmokeEvidence` schema, Rust serializer, canonical sample, and README under `docs/evidence/cross-platform-smoke/v1/`.
- Add the deterministic no-Chrome tests that guard the schema, the high-DPI wrapper flags, the per-platform configuration/skip logic, and the leak-check helper.

This story is CI-green on every lane without Chrome installed.

## Required files

- `crates/krometrail-cdp/tests/support/chrome.rs` (extend — add `ChromeWrapperVariant` + shared `ChromeWrapper`)
- `crates/krometrail-cdp/tests/support/mod.rs` (export new module(s))
- `crates/krometrail-cdp/tests/support/smoke_evidence.rs` (new — serde struct + sanitizer + schema path)
- `crates/krometrail-cdp/tests/capture_real.rs` (behavior-preserving import swap to the shared `Headless` wrapper)
- `docs/evidence/cross-platform-smoke/v1/schema.json` (new — Draft 2020-12, `additionalProperties: false`)
- `docs/evidence/cross-platform-smoke/v1/sample.json` (new — hand-authored schema-valid canonical example)
- `docs/evidence/cross-platform-smoke/v1/README.md` (new — provenance, measurements, honest non-claims, lane/convention notes)

No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion text is modified.

## Implementation notes

- The `Headless` wrapper variant must produce a byte-identical script to today's private `ChromeWrapper` so `capture_real.rs`'s done acceptance (5/5 opt-in suite) is unchanged.
- `ChromeWrapperVariant::HighDpi` prepends `--high-dpi-support=1 --force-device-scale-factor=2` between the headless flags and `"$@"`.
- `CrossPlatformSmokeEvidence` serializes exactly to the schema; the serializer enforces the same invariants the schema encodes (outcome/variant enums, percentile nullability tied to `samples == 0`, percentile ordering `p50 ≤ p95 ≤ p99`, required non-empty `non_claims`).
- The sanitizer rejects host paths outside the committed fixture constant, endpoint URLs, frame payloads, profile paths, and raw adapter error strings.
- The README names the four configurations, the `$KROMETRAIL_SMOKE_EVIDENCE_DIR` convention (default unique temp path), the per-platform lane assignment, and the explicit list of evidence that exists vs. is honestly absent (e.g. Linux Chromium when not installed).

## Acceptance criteria

- [ ] `ChromeWrapper` exists in `tests/support/chrome.rs` with `Headless` and `HighDpi` variants; `capture_real.rs` uses the shared `Headless` variant with no behavioral change to its wrapper script; the existing `capture_real.rs` opt-in suite still passes 5/5 when run with `KROMETRAIL_REAL_CHROME_TESTS=1`.
- [ ] The `HighDpi` wrapper script contains `--high-dpi-support=1` and `--force-device-scale-factor=2` (asserted by a deterministic test that reads the wrapper bytes; no Chrome needed).
- [ ] `docs/evidence/cross-platform-smoke/v1/schema.json` validates the committed `sample.json` and every `CrossPlatformSmokeEvidence` produced by the serializer (deterministic round-trip test; `additionalProperties: false`).
- [ ] The sanitizer guarantees no evidence field contains a host filesystem path outside the committed fixture path constant, an endpoint URL, a frame payload, a profile path, or a raw adapter error string (deterministic property test over the serializer outputs).
- [ ] `kind`, `schema_version`, `provenance.configuration_name`, `provenance.platform`, `provenance.cdpkit_version`, `shutdown.outcome`, and `non_claims` are required and non-empty.
- [ ] No production code, no `src/cli.rs` change, no new fixture, and no final5 file is modified.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass; `capture_real.rs` opt-in suite remains green when Chrome is available.

## Execution

- Effective worker: highest.
- No depends_on; this is the root of the smoke subtree.
