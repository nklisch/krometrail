---
id: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-real-evidence
kind: story
stage: implementing
tags: [browser, testing, infra]
parent: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke
depends_on: [epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-harness]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Real-browser cross-platform smoke and per-configuration evidence

## Scope

Add the opt-in real-Chrome smoke and the deterministic per-platform skip/leak-helper tests, then run the smoke on each supported lane and commit the schema-valid evidence documents. This story delivers Unit 2 of the feature design.

Configurations (cfg-gated):

- `target_os = "linux"` → `linux-chrome` (always) and `linux-chromium` (when Chromium is discovered; else honest `not_installed` skip).
- `target_os = "macos"` → `macos-chrome-default-dpi` and `macos-chrome-high-dpi` (`ChromeWrapperVariant::HighDpi`).
- other platforms → skip with a printed `wrong_platform` reason.

Each configuration runs two managed sessions against the existing `tests/fixtures/browser/cdp-transport-gate` fixture:

1. **Fidelity session** — identity, ≥ 30 non-empty JPEG frames, three clocks, strict per-target `CaptureOrdinal`, scaling (high-DPI band + distinct-from-default), cadence/ack histograms as diagnostics, declared-gap honesty, managed stop + leak check.
2. **Loss-reporting session** — capacity-one queue + blocked sink declares target-owned `IngestionQueueSaturated` with a positive missing estimate; release drains; bounded managed stop + leak check.

This story is CI-green on every lane without Chrome; the opt-in real-Chrome tests skip cleanly when `KROMETRAIL_REAL_CHROME_TESTS != 1` or no Chrome is discovered.

## Required file

- `crates/krometrail-cdp/tests/cross_platform_smoke.rs` (new)

Plus, after running the smoke on each lane, the operator commits:

- `docs/evidence/cross-platform-smoke/v1/linux-chrome.json`
- `docs/evidence/cross-platform-smoke/v1/linux-chromium.json` (when Chromium evidence is available)
- `docs/evidence/cross-platform-smoke/v1/macos-chrome-default-dpi.json`
- `docs/evidence/cross-platform-smoke/v1/macos-chrome-high-dpi.json`

No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified.

## Implementation notes

- Reuse the production `ProductionBrowserConnector`, the existing fixture bytes (served via the `FixtureServer` pattern or `file://` via `support::chrome::fixture_url()`), the real-browser lock, the temporary profile guard, `assert_profile_unreferenced`, and the `TestSink`/`TestClock`/`TestIds` ports.
- Reuse `assert_frame_fidelity` and `assert_strict_ordinals_by_target` — extract to `tests/support/smoke_assertions.rs` only if both test files use them unchanged; otherwise keep a scoped copy in this file. Pick the smallest non-duplicative choice.
- Per-configuration evidence writes `<configuration_name>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default `std::env::temp_dir().join(format!("krometrail-smoke-{}-{}", config, pid))`); each emitted document is asserted schema-valid in-test before the run passes.
- The high-DPI scaling assertion uses a `≥ 1.5` band and a strict greater-than comparison against the default-DPI scale recorded in the same evidence pass — never an exact `2.0`.
- Ack/cadence histograms are recorded as diagnostics with the percentile-ordering (`p50 ≤ p95 ≤ p99`) and sample-count invariants only; assert no host-speed percentile threshold.
- The incomplete-stop and proxy-sever reconnect scenarios stay in `capture_real.rs`; do not re-run them here.
- Real Chrome on these runners may emit no `Page.screencastVisibilityChanged` transition; record the count, infer no silence gap, and apply the existing conditional visibility assertion only if a transition appears.

## Acceptance criteria

- [ ] On a Linux lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs the `linux-chrome` fidelity + loss-reporting sessions and writes a schema-valid `linux-chrome.json`; if Chromium is also installed, `linux-chromium.json` is produced, otherwise the run records `not_installed` and continues.
- [ ] On a macOS lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs `macos-chrome-default-dpi` and `macos-chrome-high-dpi`, writes both schema-valid JSON documents, and the high-DPI document records `device_scale_factor ≥ 1.5` strictly greater than the default-DPI document's scale.
- [ ] Every fidelity session produces ≥ 30 non-empty JPEG frames with unique `FrameId`, strict per-target `CaptureOrdinal`, valid JPEG dimensions matching `metadata.image()`, positive viewport, finite positive scale, three clocks preserved (`session_time ≤ observed_time`, nondecreasing, `observed ≥ session_origin`), and runtime identity whose product class matches the discovered installation.
- [ ] Every loss-reporting session declares a target-owned `IngestionQueueSaturated` gap with a positive missing estimate, satisfies the `received/acknowledged/accepted/dropped` counter invariants with `dropped > 0`, drains after release, and ends in a bounded `ManagedBrowserClosed` stop with one flush.
- [ ] No ack/cadence percentile threshold is asserted; histograms are recorded as diagnostics with the percentile-ordering and sample-count invariants only.
- [ ] Every run ends with `process_references_after` and `profile_references_after` both empty; no managed Chrome process or profile reference outlives its session.
- [ ] The committed `docs/evidence/cross-platform-smoke/v1/{linux-chrome,macos-chrome-default-dpi,macos-chrome-high-dpi}.json` documents are schema-valid and carry the recorded measurements; `linux-chromium.json` is committed when Chromium evidence is available and otherwise noted as absent in the README.
- [ ] The deterministic no-Chrome tests pass on every lane; the opt-in real-Chrome tests skip cleanly (printing the reason) when Chrome or the gate is unavailable.
- [ ] No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified by this story.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Execution

- Effective worker: highest.
- Depends on `...-smoke-harness` for the shared wrapper, serializer, and schema.
