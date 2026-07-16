---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-rust-installer-gate
kind: story
stage: done
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: 1.0.0
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

- [x] The installer cannot download or execute `v0.2.20` or an older release.
- [x] A failed executable check cannot replace an existing installation or report success.
- [x] A synthetic post-cutoff Rust release installs successfully in isolation.
- [x] Current documentation advertises only truthful installation paths.
- [x] Rust, distribution, and docs gates pass.

## Review origin

Filed from the operator-authorized third GPT-5.6 Sol adversarial feature review.

## Implementation notes

- Execution capability: highest-tier direct implementation; the caller prohibited questions and subagents, so ownership remained inline.
- Review weight: maximum requested by the caller; implementation stops at `stage: review` for the independent review lane.
- Files changed: `scripts/install.sh`, `tests/installer-fixtures.sh`, `tests/distribution-static.sh`, `.github/workflows/ci.yml`, `README.md`, `docs/agents.md`, `docs/index.md`, `docs/guide/installation.md`, `docs/guide/development.md`, and generated `docs/public/llms-full.txt`.
- Tests added: hermetic installer fixtures for latest and explicit legacy rejection, checksum-valid non-executable failure with prior-install preservation, and synthetic post-cutoff success without latest-release network access.
- Verification: POSIX/Bash shell syntax, installer fixtures, distribution contracts, `bun run docs:build`, `cargo fmt --all -- --check`, locked workspace check/test/clippy gates.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran POSIX syntax, the hermetic installer/distribution suite, and all 41 workspace tests, and spot-checked pre-replacement execution validation and truthful source-only docs. Both installer findings are resolved. Verdict: Approve - story verified by implement; fast-lane advance.
