---
id: epic-agent-browser-operation-page-observation-screenshots-live-observation
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-page-observation
depends_on: [epic-agent-browser-operation-page-observation-snapshot-references]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Capture screenshots and compose live observations

## Checkpoint

Implement Unit 4 of the parent design. Add viewport, full-page, reference-element, selector-element, viewport-region, and document-region `Page.captureScreenshot` mapping with exact CSS-space conversion and target/attachment/timing/device-scale/rectangle provenance. Reference elements must use the one shared generation resolver; selectors remain marked as weaker one-shot provenance.

Reuse the existing bounded PNG/JPEG header reader by making it crate-visible. Bound base64 before allocation, reject empty/malformed/format-mismatched/oversized output, and never add a second decoder or the `image` crate. Explicit regions must lie wholly within the declared viewport/content extent; do not silently clamp or stretch them.

Compose `observe_live` from the same inspection, snapshot, and viewport screenshot functions. Bind once to a target attachment, retain per-component and aggregate observation windows, and return every component as available or as a stable structured failure. A target binding failure remains an operation error; a later transport/attachment loss makes remaining parts unavailable rather than switching targets. Do not touch the continuous screencast lifecycle or use recorder frames as current screenshots.

## Required files

- `crates/krometrail-cdp/src/control/screenshot.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/capture/image_header.rs`
- `crates/krometrail-cdp/src/capture/mod.rs`

## Acceptance evidence

- [ ] Every screenshot target maps to the exact requested CDP clip/captureBeyondViewport behavior and returns header-validated metadata and bytes.
- [ ] Viewport-to-document conversion uses fresh layout offsets; out-of-space requests fail rather than clamp.
- [ ] Reference screenshots cannot bypass stale/actionability checks; selector screenshots cannot create durable identity.
- [ ] Measured device scale and encoded dimensions are preserved independently and honestly, including scale `1.0` when a host ignores a high-DPI flag.
- [ ] Live observation preserves available evidence and actionable failures for each part while remaining on one target attachment.

## Ordering

Depends on `epic-agent-browser-operation-page-observation-snapshot-references`. It completes the reusable current-state evidence result consumed by later state-changing operations.
