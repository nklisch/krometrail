---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-linux-compatibility
kind: story
stage: review
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

- [x] Linux assets do not inherit a rolling runner's glibc minimum.
- [x] Both Linux architecture artifacts execute successfully before publication.
- [x] Installer asset mappings remain unchanged.
- [x] Rust, distribution, and docs gates pass.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review. Static musl output is selected over narrowing the product's general Linux claim to one distribution generation.

## Implementation notes

- Files changed: `.github/workflows/release.yml`, `Cross.toml`, `tests/distribution-static.sh`, `scripts/install.sh`, `README.md`, `docs/agents.md`, `docs/index.md`, `docs/guide/development.md`, `docs/guide/installation.md`, and generated `docs/public/llms-full.txt`.
- Dependency verification: `.work/bin/work-view --stage done --paths` confirmed `epic-rust-cdp-capture-foundation-rust-runtime-contracts-release-provenance.md` at terminal `stage: done` before implementation.
- Distribution: Linux release rows now use `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` on `ubuntu-24.04`; `Cross.toml` pins the cross 0.2.5 toolchain images by digest. Public asset names and immutable `RELEASE_SHA` checkout/publication provenance are unchanged. macOS and Windows matrix rows remain unchanged.
- Smoke gates: every matrix artifact runs `--version` before attestation/upload; Linux uses digest-pinned architecture-matched Alpine containers, with explicit `docker/setup-qemu-action` arm64 emulation.
- Source verification: `houseabsolute/actions-rust-cross` v1.0.8 `action.yml` and README were checked at commit `21b0f18dc621b25bfae556ff2791fca4173121e8`; cross upstream reports v0.2.5 as current and documents the musl targets; Docker QEMU v3 `action.yml` was checked at commit `c7c53464625b32c7a7e944ae62b3e17d2b600130`.
- Tests: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked` (40 tests), `cargo clippy --workspace --all-targets --locked -- -D warnings`, `bash tests/distribution-static.sh`, shell syntax checks, workflow YAML parsing, `bun run docs:build`, and `cargo run -- --version/--help` passed.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Release actions: no tag, push, or release was created. `.pi/` was not edited or staged.
- Dispatch: direct local reads and implementation only, per caller instruction; no questions or subagents.
