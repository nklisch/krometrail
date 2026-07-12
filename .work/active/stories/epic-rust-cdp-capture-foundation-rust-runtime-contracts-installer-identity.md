---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-installer-identity
kind: story
stage: implementing
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-rust-installer-gate]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Verify installer product and version identity

## Scope

Complete pre-replacement installer validation by requiring the downloaded executable to identify itself as the selected Krometrail version. Add direct coverage for binary-download failure and successful latest-resolution installation while preserving all prior cutoff, cleanup, and replacement guarantees.

## Requirements

- Require exact `--version` output `krometrail <selected-semver>` before moving the temporary artifact into place.
- Reject empty output, wrong product name, and wrong version; preserve an existing installation and clean temporary files on each failure.
- Add a direct failed-asset-download fixture that proves cleanup and old-install preservation.
- Make latest-release fixture responses configurable and add a post-cutoff latest success path without explicit `--version`.
- Preserve POSIX shell compatibility, legacy cutoff, checksums, immutable release behavior, and truthful docs.

## Acceptance criteria

- [ ] Only the requested Krometrail version can replace the installed binary.
- [ ] Empty, wrong-product, wrong-version, and failed-download paths are hermetically covered.
- [ ] Latest-resolved post-cutoff installation succeeds in isolation.
- [ ] Existing cutoff, preservation, Rust, distribution, and docs gates pass.

## Review origin

Filed from the operator-authorized fourth GPT-5.6 Sol adversarial feature review; includes both remaining GLM installer coverage nits.
