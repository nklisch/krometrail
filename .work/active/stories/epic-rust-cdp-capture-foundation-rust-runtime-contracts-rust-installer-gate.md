---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-rust-installer-gate
kind: story
stage: implementing
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Prevent the installer from serving the legacy runtime

## Scope

Ensure the public installer never presents the preserved `v0.2.20` TypeScript/DAP release as the current Rust product and never replaces a working installation with a binary that cannot execute. Until a Rust GitHub release newer than `v0.2.20` exists, installation from releases must fail clearly and current documentation must direct contributors to build from source.

## Requirements

- Reject explicit and latest-resolved release versions at or below `v0.2.20` before downloading an artifact; keep the cutoff centralized and explained as the immutable legacy boundary.
- Validate the downloaded temporary binary with `--version` before moving it over the installed path.
- On validation failure, exit non-zero, remove temporary files, preserve any prior installation, and print no success claim.
- Add isolated installer fixtures for latest legacy resolution, explicit legacy version, checksum-valid non-executable artifact, prior-install preservation, and a synthetic post-cutoff Rust release success path.
- Roll README and installation/development docs forward: no Rust GitHub release exists yet; current Rust use is source build/developer install only. Do not cut or publish a release.

## Acceptance criteria

- [ ] The installer cannot download or execute `v0.2.20` or an older release.
- [ ] A failed executable check cannot replace an existing installation or report success.
- [ ] A synthetic post-cutoff Rust release installs successfully in isolation.
- [ ] Current documentation advertises only truthful installation paths.
- [ ] Rust, distribution, and docs gates pass.

## Review origin

Filed from the operator-authorized third GPT-5.6 Sol adversarial feature review.
