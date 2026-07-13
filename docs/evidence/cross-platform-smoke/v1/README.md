# Cross-platform capture smoke evidence

This directory holds the schema, a canonical sample, and the manual evidence policy for the
production-path capture fidelity smoke. The smoke proves that the current Krometrail capture
pipeline (`ProductionBrowserConnector` + cdpkit 0.4.0 + supervised targets + bounded ingestion)
captures frames, reports loss honestly, and cleans up after itself on supported platform/Chrome
combinations.

## Files

- `schema.json` — Draft 2020-12 JSON Schema for `CrossPlatformSmokeEvidence`.
- `sample.json` — Hand-authored, schema-valid canonical example pinned to the serializer's
  canonical byte layout. The deterministic round-trip test asserts that
  `serde_json::to_vec_pretty(&CrossPlatformSmokeEvidence::sample())` equals this file byte-for-byte.
- `linux-chrome.json` — Decisive Linux Chrome evidence from revision
  `595f079ce9772e3974f65bdc62ad70b01c17dbce`.
- `macos-chrome-default-dpi.json` — Decisive macOS default-DPI evidence from revision
  `4dfc78b2a6e91c3404fc58c3c8b98c5b6d662fdc` and hosted run `29288634536`.
- `README.md` — This file: provenance, measurements, honest non-claims, lane/convention notes,
  exact manual commands, and the decisive/skip policy.

## What this smoke proves

- A managed Chrome launch through the shared headless wrapper starts, reaches `Ready`, and records
  runtime browser/protocol identity.
- At least 30 non-empty JPEG frames are captured with strict per-target `CaptureOrdinal`
  continuity, non-decreasing observed/session clocks, and JPEG dimensions matching
  `metadata.image()`.
- A capacity-one, blocked-sink session declares `IngestionQueueSaturated` honestly with a positive
  missing-frame estimate and drains accepted work before a bounded managed stop.
- No managed Chrome process or temporary profile reference outlives its session on Linux
  (`/proc/*/cmdline`) and macOS (`ps -ax -o pid= -o command=`).
- Default-DPI and high-DPI macOS configurations are both exercised; both force device scale so the
  observed band is host-independent.

## Honest non-claims

Every evidence document carries a `non_claims` array repeating these limits:

- **No transport requalification.** The cdpkit 0.4.0 selection, 60 s / 1 000-frame sustained
  saturation, deterministic wire assertions, and ack p99/max thresholds are owned by
  `docs/evidence/cdp-transport/v2/`.
- **No host-speed percentile threshold.** Ack/cadence histograms are diagnostics with internal
  consistency checks only (samples ≥ N, percentiles monotonic); no `p99 ≤ X ms` claim is made.
- **No product-thesis capture-probability threshold.** The 100 ms / 95 % and 50 ms / 80 % capture
  envelope and the 25 pp agent-improvement bar belong to the evaluation epic.
- **No duration sweep, visual-defect corpus, artifact comparison, or storage validation.** Those
  are owned by the evaluation epic.
- **No claim that Chrome's acknowledgement token changes, orders frames, or detects skipped browser
  frames.**

## Measurements

Each session records:

- `frame_count`, `source_time_samples`, `image_dimensions`, `viewport`, `device_scale_factor`
- `capture_ordinal_range.min` and `.max`
- `observed_clock_span_nanos`, `session_clock_span_nanos`
- `ack_latency_nanos` and `frame_cadence_nanos` as `{samples, p50, p95, p99, max}`
- Declared gaps as `{reason, count}`
- `visibility_events` count (usually zero)

The provenance block records git revision, Rust toolchain, cdpkit version, discovered installation,
runtime `BrowserVersion`, the forced-DPI wrapper variant, and a snapshot of the actual `CaptureConfig`
used (fidelity uses `CaptureConfig::default()` verbatim; loss-reporting overrides only
`queue_capacity` to `1`).

## Lane/convention notes

- Evidence is written by the test to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default: a unique temp path).
  The test never mutates the source tree during `cargo test`; the operator commits the produced JSON.
- The smoke is opt-in via `KROMETRAIL_REAL_CHROME_TESTS=1`.
- `ChromeWrapperVariant::DefaultDpi` forces `--force-device-scale-factor=1`;
  `ChromeWrapperVariant::HighDpi` adds `--high-dpi-support=1 --force-device-scale-factor=2`.
- Linux Chrome and Linux Chromium are selected explicitly by `BrowserProduct`, not by
  first-discovered order.

## Manual evidence procedure

### Linux lane (Chrome required; Chromium optional)

```bash
KROMETRAIL_REAL_CHROME_TESTS=1 \
KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 \
cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored
```

This writes `linux-chrome.json` always and `linux-chromium.json` only when a
`BrowserProduct::Chromium` installation is discovered. If Chromium is absent, the run prints
`not_installed` and continues.

### macOS lane (Chrome required)

```bash
KROMETRAIL_REAL_CHROME_TESTS=1 \
KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 \
cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored
```

This writes both `macos-chrome-default-dpi.json` and `macos-chrome-high-dpi.json`.

## Decisive/skip policy

- **Decisive configurations** (must be present and green): `linux-chrome`,
  `macos-chrome-default-dpi`, `macos-chrome-high-dpi`.
- **Best-effort configuration** (honest skip permitted): `linux-chromium`.
- **Wrong platform:** the test skips with a printed reason on any `target_os` outside
  `{linux, macos}`.
- **Skip, not fail:** when `KROMETRAIL_REAL_CHROME_TESTS != 1` or no decisive installation is
  discovered, the test skips cleanly with the exact reason printed to `--nocapture` output.

## Evidence that exists vs. honestly absent

- `linux-chrome.json` is present and passed both fidelity and capacity-one loss-reporting sessions
  against Chrome 149.0.7827.155 on Linux x86_64.
- `macos-chrome-default-dpi.json` is present and passed both sessions against Chrome 149.0.7827.201
  on hosted macOS 14 arm64. Its process/profile cleanup result used the real `ps`-based reference
  scan.
- `macos-chrome-high-dpi.json` is intentionally absent. Hosted runs `29288505121` and `29288634536`
  both observed `deviceScaleFactor == 1.0` under
  `--high-dpi-support=1 --force-device-scale-factor=2`; the decisive `>= 1.5` assertion failed, so
  the harness emitted no passing document. The operator explicitly authorized deferring this
  evidence on 2026-07-13 so the foundation implementation can continue without weakening the gate
  or fabricating a pass. A future lane must either demonstrate a Chrome configuration that exposes
  high DPI through the production capture metadata or redesign that capture boundary before this
  evidence can be added.
- `linux-chromium.json` is honestly absent because no `BrowserProduct::Chromium` installation was
  available on the Linux lane; this configuration remains best-effort.
- The temporary publication used branch `tmp/cross-platform-smoke-macos-595f079`, workflow
  `CDP transport macOS gate`, workflow patch commit
  `a99d18e1c0bb8e07fed80c278cc05e59494bb21a`, and evidence commit
  `4dfc78b2a6e91c3404fc58c3c8b98c5b6d662fdc`. Run `29288505121` first exposed the high-DPI
  failure; run `29288634536` retained and uploaded the valid default-DPI document before the same
  decisive high-DPI failure.
- Real visibility transitions and `Page.screencastVisibilityChanged` events are observed and counted,
  not assumed.
