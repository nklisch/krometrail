---
id: gate-security-pin-release-action-shas
kind: story
stage: implementing
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
