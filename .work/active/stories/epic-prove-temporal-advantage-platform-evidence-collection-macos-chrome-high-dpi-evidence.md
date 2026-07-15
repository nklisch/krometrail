---
id: epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-high-dpi-evidence
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

# Collect macOS stable-Chrome high-DPI evidence

## Checkpoint

Collect and validate an independently identified macOS stable-Chrome high-DPI run. A wrapper flag
or prior smoke attempt is not evidence; the production capture metadata must observe the required
scale band.

## Exact implementation

Implement `run_macos_high_dpi` in `src/app/platform_evidence.rs` through the shared live
composition. Request the existing high-DPI wrapper variant with canonical requested scale two,
then require macOS stable Chrome, 800x450, and observed device scale at least 1.5 before accepting
the row. Preserve exact source/observed/session clocks, gaps, capture/fixture/seed/threshold
identity, browser/protocol identity, cleanup, and non-claims in the existing manifest.

The lane has an internal sequence of preflight, launch, observed scale barrier, qualification,
manifest validation, cleanup, and ignored-output publication. It is independent of default-DPI at
the graph level, although the real-browser lock serializes actual launches. A scale-one result is
blocked/inconclusive and produces no passing high-DPI document, matching the absent historical
smoke evidence.

## Acceptance evidence

- [ ] High-DPI pass requires observed production scale at least 1.5; requested flags alone cannot
      satisfy the lane.
- [ ] Scale one, missing macOS/Chrome, unsupported protocol, capture gap, retention failure, or
      cleanup failure is explicit blocked/inconclusive evidence and never a pass.
- [ ] The lane cannot block the Linux reference-host or manual interpretation dependency.
- [ ] Tests prove the prior wrapper-only/observed-one scenario is rejected without launching Chrome.

## Ordering and blocker

Depends only on the shared lane contract. Operator authorization and a host that exposes the
required scale are needed; no high-DPI claim is made until that evidence exists.
