---
id: epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-default-dpi-evidence
kind: story
stage: implementing
tags: [testing, browser, infra, visual]
parent: epic-prove-temporal-advantage-platform-evidence-collection
depends_on: [epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Collect macOS stable-Chrome default-DPI evidence

## Checkpoint

Collect a separate macOS stable-Chrome default-DPI live qualification result for the platform
matrix. This lane is independent of Linux reference-host and high-DPI completion.

## Exact implementation

Implement `run_macos_default_dpi` in `src/app/platform_evidence.rs` through the shared production
live harness. Require macOS, stable Chrome selected explicitly, 800x450, and observed scale one.
The existing default-DPI wrapper variant is only a request; `RunManifest` capture metadata is the
authority. Validate browser/protocol identity, platform/architecture, fixture and capture
identity, complete qualification gates, source availability, and cleanup before publishing the
lane digest.

The committed cross-platform smoke document is prerequisite context and not a replacement for
this feature's benchmark identity or live qualification profile. If macOS or Chrome is unavailable,
or the run cannot meet the observation contract, preserve a blocked/inconclusive record and do not
block Linux manual evaluation.

## Acceptance evidence

- [ ] A decisive row has observed macOS stable Chrome, default-DPI scale one, canonical viewport,
      complete live qualification, exact manifest identity, and safe ignored output.
- [ ] Wrong platform, missing browser, unsupported protocol, gaps, retention loss, failed cleanup,
      or incomplete sample coverage remains non-passing with recovery information.
- [ ] The lane does not infer evidence from Linux or from the prior default-DPI smoke artifact.
- [ ] A missing macOS row leaves the future cross-platform assessment inconclusive without blocking
      the reference-host or manual interpretation paths.

## Ordering and blocker

Depends only on the shared lane contract. Operator authorization and a macOS host are required
for decisive evidence; no Chrome is launched during design or ordinary verification.
