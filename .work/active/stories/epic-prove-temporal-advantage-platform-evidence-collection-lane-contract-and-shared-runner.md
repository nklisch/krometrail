---
id: epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner
kind: story
stage: done
tags: [testing, browser, infra]
parent: epic-prove-temporal-advantage-platform-evidence-collection
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Establish the platform lane contract and shared runner

## Checkpoint

Create the one registry and shared test-only runner that all platform lanes use. Make lane
identity, required/optional status, browser product, platform, viewport, and observed-DPI rules
explicit before any lane can publish evidence.

## Exact implementation

Add `crates/temporal-evaluation/src/platform.rs` and export it from `lib.rs`. Register exactly:

- `LinuxStableChromeReferenceHost` — required Linux stable Chrome, default-DPI, reference host;
- `MacosChromeDefaultDpi` — required macOS stable Chrome, default-DPI;
- `MacosChromeHighDpi` — required macOS stable Chrome, observed scale at least 1.5;
- `LinuxChromiumOptional` — optional Linux Chromium, default-DPI.

The registry owns `ALL` and `REQUIRED` order plus each lane's expected environment, product, scale
band, and claim role. Add `PlatformLaneConfig` and the platform profile/lane declaration needed
by `src/app/platform_evidence.rs` to invoke the existing `run_live_qualification` composition.
Extend the current `RunManifest` contract in place only where the platform profile and declared
lane identity are required; regenerate schemas through the existing generators.

The shared runner must reuse the production connector, one `RecordingStore` graph, existing
qualification support, real-browser lock, fixture server, cleanup, and ignored output boundary.
It may not add a product command, model call, browser download, network fallback, or second
manifest/store/artifact authority. A wrapper flag is never accepted as DPI evidence without the
observed capture metadata.

## Acceptance evidence

- [ ] Registry, generated schema, and canonical bytes are deterministic and reject unknown,
      duplicate, wrong-platform, wrong-product, wrong-profile, wrong-viewport, and wrong-scale
      lane declarations.
- [ ] The shared runner has no side effects before both explicit opt-in gates, uses one production
      authority graph per run, and records cleanup/failure honestly.
- [ ] The high-DPI validator rejects observed scale one even when high-DPI flags were requested.
- [ ] Lane definitions and claim text are not duplicated in runner branches or tests.

## Ordering

This is the shared prerequisite for the four lane checkpoints. It depends on completed live
capture/system qualification and does not itself require Chrome during ordinary verification.

## Operator blockers

Implementation and default tests do not collect evidence. An operator must later authorize each
required live lane and provide its local browser installation; macOS may remain unavailable.

## Implementation notes

- Added the sole registry in `crates/temporal-evaluation/src/platform.rs`, with the exact four
  lanes, canonical `ALL`/`REQUIRED` order, typed profile, scale band, viewport, environment,
  product, and claim role. `validate_platform_lane` validates an existing manifest without reading
  files or launching a browser; requested wrapper scale is never used as observed evidence.
- Added the optional typed `RunManifest.platform` declaration and registered
  `platform-evidence-v1` profile. Existing non-platform contract/live manifests remain valid;
  platform manifests use the registry-owned platform non-claims and existing qualification
  measurements for observed facts. Regenerated `run-manifest.schema.json` and updated its digest
  assertion through `generate-run-manifest`.
- Added test-only `src/app/platform_evidence.rs`. It derives every live configuration value from
  the lane registry, checks both opt-in gates before any validation/discovery/resource boundary,
  invokes the existing `run_live_qualification` composition, and validates the returned manifest.
  The existing lifecycle, lock, fixture server, production connector, single `RecordingStore`
  graph, cleanup, and ignored output boundary remain authoritative.
- Generalized the existing qualification viewport/wrapper and fixture predicates to carry the
  declared lane scale while normalizing high-DPI fixture pixels to the fixed CSS viewport for
  observation. This does not alter source-frame authority or accept a wrapper flag as proof.
- Added deterministic/privacy/canonical platform tests, including unknown/wrong identity
  rejection, high-DPI requested-scale versus observed-scale-one rejection, explicit non-passing
  cleanup/failure behavior, and disabled-run side-effect protection. No Chrome, model, download,
  network fallback, or live evidence was used.

## Verification

- `rustup run 1.85.0 cargo fmt --all -- --check`
- `rustup run 1.85.0 cargo check --workspace --all-targets --locked`
- `rustup run 1.85.0 cargo test --workspace --all-targets --locked`
- `rustup run 1.85.0 cargo clippy --workspace --all-targets --locked -- -D warnings`
- The same workspace check/test/clippy gates passed with `--features qualification-support`,
  with both live opt-in environment variables unset. The ignored live tests were not run.
