---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-cutover
kind: story
stage: done
tags: [infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal]
release_binding: 1.0.0
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

- [x] Rust CI covers the complete quality gate.
- [x] Static workflow tests prove every expected asset, checksum, and installer mapping.
- [x] No npm publish or Bun product build remains.
- [x] Cargo is the only product version source and `--version` reflects it.
- [x] Installer shell syntax/checksum behavior and developer install pass.

## Implementation notes

- Dispatch: direct local reads only, per caller; no subagents or questions.
- Files changed: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.gitignore`, `scripts/install.sh`, `scripts/dev-install.sh`, `scripts/bump-version.ts`, `tests/distribution-static.sh`.
- Release builds use native GitHub-hosted runners for the five explicit Rust target/asset rows, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, per-asset provenance attestations, and a fail-fast checksum/release aggregation step. The existing Pages workflow remains docs-only and does not compile the product.
- The installer now has an explicit four-platform mapping, preserves the public Windows download reference, and rejects missing, malformed, or unverifiable checksums instead of continuing.
- Versioning reads the root Cargo `[package].version`; Cargo workspace metadata is synchronized for inherited member versions, while `package.json` remains private and docs/fixture-only. `--prepare` and `--dry-run` make bump behavior testable without commit, tag, or push side effects.
- Tests added: `tests/distribution-static.sh` validates all asset/checksum/installer mappings, Rust CI commands, package/version ownership, developer-install path, and isolated bump behavior.
- Verification: `cargo fmt --all --check`; locked workspace check/test/clippy with denied warnings; shell syntax checks; static distribution tests; `cargo run -- --version`, `--help`, and unavailable `doctor`; isolated `scripts/dev-install.sh` install/version check.
- Discrepancies from design: `Cargo.lock` required no content change; `package.json` already satisfied the private docs/fixture-only contract; `deploy-pages.yml` was already isolated to VitePress docs.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran Rust formatting, all 29 workspace tests, clippy, installer shell syntax, and the actual `tests/distribution-static.sh` contract suite successfully. An initial orchestrator command named a nonexistent Cargo test target; this was a review-command mistake, not an implementation failure, and the intended static suite passed. Verdict: Approve - story verified by implement; fast-lane advance.
