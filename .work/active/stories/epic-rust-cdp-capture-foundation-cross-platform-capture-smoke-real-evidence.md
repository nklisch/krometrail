---
id: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke-real-evidence
kind: story
stage: done
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

Add the opt-in real-Chrome smoke and the deterministic per-platform skip/leak-helper tests, then run the smoke on each supported lane and commit the schema-valid evidence documents. This story delivers Unit 2 of the feature design (revised by the 2026-07-13 design-repair pass).

Configurations (cfg-gated), each selecting its installation **explicitly by `BrowserProduct`** rather than "first discovered":

- `target_os = "linux"` → `linux-chrome` (always; selects `BrowserProduct::Chrome`) and `linux-chromium` (when a `BrowserProduct::Chromium` installation is discovered; else honest `not_installed` skip — never a failure).
- `target_os = "macos"` → `macos-chrome-default-dpi` (`ChromeWrapperVariant::DefaultDpi`, forces scale 1) and `macos-chrome-high-dpi` (`ChromeWrapperVariant::HighDpi`, forces scale 2).
- other platforms → skip with a printed `wrong_platform` reason.

Each configuration runs two managed sessions against the existing `tests/fixtures/browser/cdp-transport-gate` fixture:

1. **Fidelity session** — uses `CaptureConfig::default()` verbatim (one explicit config: queue_capacity 4, max_active_streams 8, shutdown_timeout 5 s, ack_timeout 250 ms, jpeg_quality 80, format jpeg, gap_ledger_capacity 64, max_base64_payload_bytes 8 MiB). Asserts identity, ≥ 30 non-empty JPEG frames, three clocks, strict per-target `CaptureOrdinal`, scaling (both bands asserted: default ≤ 1.5 anchored to forced 1, high ≥ 1.5 anchored to forced 2, high strictly > default), cadence/ack histograms as diagnostics, declared-gap honesty, managed stop + leak check.
2. **Loss-reporting session** — `CaptureConfig::default()` with the single justified override `queue_capacity: 1` and a blocked sink declares target-owned `IngestionQueueSaturated` with `estimated_missing_frames()` returning `Some(_)`; release drains; bounded managed stop + leak check.

This story is CI-green on every lane without Chrome; the opt-in real-Chrome tests skip cleanly when `KROMETRAIL_REAL_CHROME_TESTS != 1` or no decisive installation matches the configuration's `BrowserProduct`.

## Required file

- `crates/krometrail-cdp/tests/cross_platform_smoke.rs` (new, opening with `#![cfg(feature = "cdpkit-transport")]`)

Plus, after running the smoke on each lane, the operator commits:

- `docs/evidence/cross-platform-smoke/v1/linux-chrome.json` (decisive)
- `docs/evidence/cross-platform-smoke/v1/linux-chromium.json` (best-effort; when Chromium evidence is available)
- `docs/evidence/cross-platform-smoke/v1/macos-chrome-default-dpi.json` (decisive)
- `docs/evidence/cross-platform-smoke/v1/macos-chrome-high-dpi.json` (decisive)

No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified.

## Implementation notes

- Reuse the production `ProductionBrowserConnector`, the existing fixture bytes (served via the `FixtureServer` pattern or `file://` via `support::chrome::fixture_url()`), the real-browser lock, the temporary profile guard, `assert_profile_unreferenced`, and the `TestSink`/`TestClock`/`TestIds` ports.
- Reuse `assert_frame_fidelity` and `assert_strict_ordinals_by_target` — extract to `tests/support/smoke_assertions.rs` only if both test files use them unchanged; otherwise keep a scoped copy in this file. Pick the smallest non-duplicative choice.
- Select installations explicitly: `linux-chrome` uses `BrowserProduct::Chrome`; `linux-chromium` uses `BrowserProduct::Chromium` and is skipped with `not_installed` when absent. The wrapper is constructed from the explicitly selected executable, so the two Linux configurations cannot swap installations.
- Read runtime identity via the **field** `session.compatibility().version` (not a method), then `.product()` (compared as the same `BrowserProduct` enum against the discovered installation), `.product_version().as_str()`, `.revision()`, `.protocol_version()`, `.user_agent()`, `.js_version()`.
- Per-configuration evidence writes `<configuration_name>.json` to `$KROMETRAIL_SMOKE_EVIDENCE_DIR` (default `std::env::temp_dir().join(format!("krometrail-smoke-{}-{}", config, pid))`); each emitted document is asserted schema-valid in-test, and its `capture_config` snapshot equals the actual `CaptureConfig` value (fidelity default, loss override) before the run passes.
- The scaling assertion records **both** DPI configurations in the same evidence pass (both wrappers now force scale): default-DPI `metadata.device_scale_factor() ≤ 1.5`, high-DPI `≥ 1.5`, and high strictly greater than default. No host-native display assumption.
- Ack/cadence histograms are recorded as diagnostics with the percentile-ordering (`p50 ≤ p95 ≤ p99`) and sample-count invariants only; assert no host-speed percentile threshold.
- Leak checks use `process_references` which has a real `ps`-based branch on macOS (delivered by the harness story); on macOS evidence gathered before that scan lands, record the limitation in `non_claims` rather than asserting an unverified empty list.
- The incomplete-stop and proxy-sever reconnect scenarios stay in `capture_real.rs`; do not re-run them here.
- Real Chrome on these runners may emit no `Page.screencastVisibilityChanged` transition; record the count, infer no silence gap, and apply the existing conditional visibility assertion only if a transition appears.

## Manual evidence procedure

Run, per lane, from a clean checkout at the pinned revision:

```bash
KROMETRAIL_REAL_CHROME_TESTS=1 \
KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 \
cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture --include-ignored
```

Linux produces `linux-chrome.json` always and `linux-chromium.json` when Chromium is discovered (else `not_installed`); macOS produces both `macos-chrome-default-dpi.json` and `macos-chrome-high-dpi.json`. The operator commits the produced files and notes any honest absence in the README.

**Decisive vs. skip:** `linux-chrome`, `macos-chrome-default-dpi`, `macos-chrome-high-dpi` are decisive — all three must be present and schema-valid before the parent epic (`epic-rust-cdp-capture-foundation`) can advance to done. `linux-chromium` is best-effort (honest skip permitted). Wrong-platform and gate-unavailable are clean skips with the reason printed, never failures.

## Acceptance criteria

- [ ] On a Linux lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke selects the `BrowserProduct::Chrome` installation (filtered explicitly) for `linux-chrome`, runs the fidelity + loss-reporting sessions, and writes a schema-valid `linux-chrome.json` whose `provenance.capture_config` equals `CaptureConfig::default()`; if a `BrowserProduct::Chromium` installation is also discovered, `linux-chromium.json` is produced via the same explicit filter, otherwise the run records `not_installed` and continues.
- [ ] On a macOS lane with Chrome installed and `KROMETRAIL_REAL_CHROME_TESTS=1`, the smoke runs `macos-chrome-default-dpi` (forces scale 1) and `macos-chrome-high-dpi` (forces scale 2), writes both schema-valid JSON documents, and the high-DPI document records `device_scale_factor ≥ 1.5` strictly greater than the default-DPI document's scale (`≤ 1.5`).
- [ ] Every fidelity session produces ≥ 30 non-empty JPEG frames with unique `FrameId`, strict per-target `CaptureOrdinal`, valid JPEG dimensions matching `metadata.image()`, positive viewport, finite positive scale, three clocks preserved (`session_time ≤ observed_time`, nondecreasing, `observed ≥ session_origin`), and runtime identity (`session.compatibility().version`) whose `BrowserProduct` matches the discovered `BrowserInstallation::product`.
- [ ] Every loss-reporting session uses `CaptureConfig::default()` with only `queue_capacity: 1` overridden, declares a target-owned `IngestionQueueSaturated` gap with `estimated_missing_frames()` returning `Some(_)`, satisfies the `status.statistics()` counter invariants with `status.statistics().dropped_frames() > 0`, drains after release, and ends in a bounded `ManagedBrowserClosed` stop with one flush.
- [ ] No ack/cadence percentile threshold is asserted; histograms are recorded as diagnostics with the percentile-ordering and sample-count invariants only.
- [ ] Every run ends with `process_references_after` and `profile_references_after` both empty *and verified by a real reference scan* (Linux `/proc/*/cmdline`, macOS `ps -ax -o pid= -o command=`); macOS evidence that predates the `ps`-based scan records the limitation in `non_claims` rather than asserting an unverified empty list.
- [ ] The committed `docs/evidence/cross-platform-smoke/v1/{linux-chrome,macos-chrome-default-dpi,macos-chrome-high-dpi}.json` documents are schema-valid, carry the recorded measurements, and whose `capture_config` fields match the actual `CaptureConfig` values used; `linux-chromium.json` is committed when Chromium evidence is available and otherwise noted as absent in the README.
- [ ] The deterministic no-Chrome tests pass on every lane; the opt-in real-Chrome tests skip cleanly (printing the reason) when Chrome or the gate is unavailable.
- [ ] `cross_platform_smoke.rs` opens with `#![cfg(feature = "cdpkit-transport")]`; `cargo test -p krometrail-cdp --no-default-features --tests --locked` succeeds and does not compile the smoke.
- [ ] No production code, `src/cli.rs`, fixture content, final5 evidence, or `capture_real.rs` assertion is modified by this story.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Execution

- Effective worker: highest.
- Depends on `...-smoke-harness` for the shared wrapper, serializer, schema, `BrowserVersion` accessor test, macOS reference scan, and canonical-bytes layout.

## Implementation notes

Implemented the opt-in production-path smoke in
`crates/krometrail-cdp/tests/cross_platform_smoke.rs` as one feature-owned bundle. It now:

- selects installations explicitly by `BrowserProduct`;
- runs a default-config fidelity session and a capacity-one blocked-sink loss session;
- checks JPEG dimensions, identities, strict capture ordinals, three-clock ordering, diagnostic
  timing summaries, counter invariants, managed stop/flush, and live process/profile references;
- writes canonical schema-validated evidence to a repository-relative or absolute operator path;
- preserves completed per-configuration evidence when a later decisive configuration fails;
- validates every committed runtime evidence JSON against both Rust invariants and `schema.json`.

The implementation landed in `595f079ce9772e3974f65bdc62ad70b01c17dbce`; completed-evidence
retention landed in `048b36a` on `main` (from temporary evidence commit
`4dfc78b2a6e91c3404fc58c3c8b98c5b6d662fdc`). No production behavior, CLI surface, fixture,
or final5 evidence changed.

## Real-platform evidence and operator-authorized disposition

- Linux Chrome passed at revision `595f079ce9772e3974f65bdc62ad70b01c17dbce`, producing
  `docs/evidence/cross-platform-smoke/v1/linux-chrome.json`. Chrome 149.0.7827.155 produced 30
  fidelity frames with no declared gaps and a loss session with four explicitly declared
  `ingestion_queue_saturated` gaps. Chromium was not installed and remains an allowed best-effort
  absence.
- Hosted macOS default DPI passed at revision
  `4dfc78b2a6e91c3404fc58c3c8b98c5b6d662fdc`, producing
  `macos-chrome-default-dpi.json`. Chrome 149.0.7827.201 on arm64 produced 30 fidelity frames,
  explicit loss reporting, bounded managed shutdown, and clean `ps`-based reference scans.
- Hosted macOS high DPI did **not** pass. Runs `29288505121` and `29288634536` both observed
  `deviceScaleFactor == 1.0` even though the exact wrapper contained
  `--high-dpi-support=1 --force-device-scale-factor=2`. The decisive `>= 1.5` assertion remained
  intact; no `macos-chrome-high-dpi.json` was emitted or fabricated.
- On 2026-07-13 the operator explicitly authorized deferring that materially blocked high-DPI
  evidence so the feature and parent epic can continue. This authorization supersedes the original
  parent-approval requirement for this cycle; it does not convert the failed gate into a pass.
  Exact recovery is to demonstrate a Chrome/runner configuration that exposes scale >= 1.5 through
  production screencast metadata, or redesign that metadata boundary, then add the missing canonical
  document.

Temporary publication was limited to branch `tmp/cross-platform-smoke-macos-595f079` and workflow
`CDP transport macOS gate`; no force-push or release occurred. Workflow patch commit:
`a99d18e1c0bb8e07fed80c278cc05e59494bb21a`. Evidence commit:
`4dfc78b2a6e91c3404fc58c3c8b98c5b6d662fdc`.

## Verification

- `KROMETRAIL_REAL_CHROME_TESTS=1 KROMETRAIL_SMOKE_EVIDENCE_DIR=docs/evidence/cross-platform-smoke/v1 cargo test -p krometrail-cdp --test cross_platform_smoke --locked -- --nocapture` — Linux passed, 13/13 before the committed-evidence validation test was added; the final deterministic suite passes 14/14.
- Hosted macOS deterministic and default-DPI real smoke checks passed; high DPI failed honestly as
  recorded above. Run `29288634536` uploaded the valid default-DPI artifact before failing.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `cargo test -p krometrail-cdp --no-default-features --tests --locked` — passed during harness
  verification; the gated smoke target contained zero tests.

The unrelated pre-existing `.work/bin/work-view` modification was preserved and excluded from every
commit.
