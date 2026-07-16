---
id: gate-security-pin-release-action-shas
kind: story
stage: review
tags: [security, infra]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: security
created: 2026-07-15
updated: 2026-07-15
---

# Pin privileged release workflow actions to immutable SHAs

## Severity
High

## Domain
Installer/release CI and supply chain

## Location
`.github/workflows/release.yml:17`

## Evidence

The release workflow grants `contents: write`, `id-token: write`, and `attestations: write`, while several steps use mutable major or channel references including `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `actions/attest-build-provenance@v2`, `actions/upload-artifact@v4`, `actions/download-artifact@v4`, and `softprops/action-gh-release@v2`.

## Remediation direction

Pin every third-party and first-party action in the privileged release workflow to a reviewed full commit SHA while retaining the current action version comments, immutable tag/SHA verification, build matrix, attestations, and release functionality. Keep publication permissions scoped to the existing jobs; do not remove automated releases.

## Implementation evidence

- All `uses:` entries in `.github/workflows/release.yml` now use full immutable commit SHAs; readable channel/version comments remain inline. The build matrix, smoke tests, attestations, uploads, downloads, checksums, tag identity checks, and publication permissions are unchanged.
- Each ref was resolved from its official upstream repository:
  - `https://github.com/actions/checkout` `refs/tags/v4` -> `34e114876b0b11c390a56381ad16ebd13914f8d5`
  - `https://github.com/dtolnay/rust-toolchain` `refs/heads/stable` -> `4be7066ada62dd38de10e7b70166bc74ed198c30`
  - `https://github.com/Swatinem/rust-cache` peeled `refs/tags/v2` -> `e18b497796c12c097a38f9edb9d0641fb99eee32`
  - `https://github.com/houseabsolute/actions-rust-cross` `refs/tags/v1.0.8` -> `21b0f18dc621b25bfae556ff2791fca4173121e8`
  - `https://github.com/docker/setup-qemu-action` `refs/tags/v3` -> `c7c53464625b32c7a7e944ae62b3e17d2b600130`
  - `https://github.com/actions/attest-build-provenance` peeled `refs/tags/v2` -> `e8998f949152b193b063cb0ec769d69d929409be`
  - `https://github.com/actions/upload-artifact` `refs/tags/v4` -> `ea165f8d65b6e75b540449e92b4886f43607fa02`
  - `https://github.com/actions/download-artifact` `refs/tags/v4` -> `d3f86a106a0bac45b974a628896c90dbdf5c8093`
  - `https://github.com/softprops/action-gh-release` `refs/tags/v2` -> `3bb12739c298aeb8a4eeaf626c5b8d85266b0e65`
- `tests/distribution-static.sh` now verifies every release-workflow action reference is exactly 40 lowercase hexadecimal characters.

## Verification

- `bash -n tests/distribution-static.sh`
- `bash tests/distribution-static.sh` -> `distribution contracts: ok`
