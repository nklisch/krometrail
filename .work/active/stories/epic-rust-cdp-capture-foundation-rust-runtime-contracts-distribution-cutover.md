---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-cutover
kind: story
stage: implementing
tags: [infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Cut distribution and development tooling over to Rust

## Scope

Implement the parent feature's Unit 6: Rust CI, cross-platform release builds, stable installer behavior, Cargo-owned versioning, Cargo-based developer install, and a private docs/fixture-only Bun package. Remove npm publication and every TypeScript product build/test entry.

Preserve these public release asset names exactly: `krometrail-linux-x64`, `krometrail-linux-arm64`, `krometrail-darwin-x64`, `krometrail-darwin-arm64`, and `krometrail-windows-x64.exe`.

## Implementation requirements

- Add Rust pull-request CI: format, check, test, clippy with denied warnings.
- Build, attest, checksum, and release all five existing binary asset names; Windows remains best-effort, not a supported environment.
- Keep `scripts/install.sh` POSIX and its URLs/installed binary stable.
- Build developer installs from `target/release/krometrail`.
- Root Cargo package version is the sole product version; the bump script updates exactly it.
- `package.json` is private and contains only still-used docs/browser-fixture tooling, with no version mirror, product entry point, build, test, or publish surface.
- Pages/docs tooling remains isolated from product compilation and does not generate old product APIs.

## Acceptance criteria

- [ ] Rust CI covers the complete quality gate.
- [ ] Static workflow tests prove every expected asset, checksum, and installer mapping.
- [ ] No npm publish or Bun product build remains.
- [ ] Cargo is the only product version source and `--version` reflects it.
- [ ] Installer shell syntax/checksum behavior and developer install pass.
