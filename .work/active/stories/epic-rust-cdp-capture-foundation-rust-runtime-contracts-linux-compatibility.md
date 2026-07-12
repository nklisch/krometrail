---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-linux-compatibility
kind: story
stage: implementing
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-release-provenance]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make the Linux release compatibility contract explicit

## Scope

Resolve the unbounded Linux-support claim found by the second adversarial review without weakening the supported-environment promise to a rolling CI runner. Produce the existing Linux asset names as static musl binaries for x64 and arm64, and smoke-test the produced artifacts in Linux environments before release publication.

## Requirements

- Build the existing `krometrail-linux-x64` and `krometrail-linux-arm64` assets for musl targets using a maintained reproducible cross-build mechanism.
- Execute each produced binary's `--version` contract in a matching architecture environment before upload; emulation is acceptable for arm64 when native capacity is unavailable.
- Document the static Linux binary baseline in current development/install documentation without changing asset names.
- Extend distribution contracts to reject GNU/rolling-runner Linux release rows and missing artifact smoke tests.

## Acceptance criteria

- [ ] Linux assets do not inherit a rolling runner's glibc minimum.
- [ ] Both Linux architecture artifacts execute successfully before publication.
- [ ] Installer asset mappings remain unchanged.
- [ ] Rust, distribution, and docs gates pass.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review. Static musl output is selected over narrowing the product's general Linux claim to one distribution generation.
