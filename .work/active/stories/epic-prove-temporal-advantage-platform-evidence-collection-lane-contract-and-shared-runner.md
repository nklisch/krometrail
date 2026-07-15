---
id: epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner
kind: story
stage: implementing
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
