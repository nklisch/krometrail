---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-installer-identity
kind: story
stage: review
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

- [x] Only the requested Krometrail version can replace the installed binary.
- [x] Empty, wrong-product, wrong-version, and failed-download paths are hermetically covered.
- [x] Latest-resolved post-cutoff installation succeeds in isolation.
- [x] Existing cutoff, preservation, Rust, distribution, and docs gates pass.

## Review origin

Filed from the operator-authorized fourth GPT-5.6 Sol adversarial feature review; includes both remaining GLM installer coverage nits.

## Implementation notes

- Execution capability: highest-tier direct implementation; installer and hermetic fixture ownership stayed inline per the caller's no-questions/no-subagents constraint.
- Review weight: maximum requested by the active autopilot caller; implementation stops at `stage: review` for the review lane.
- Files changed: `scripts/install.sh`, `tests/installer-fixtures.sh`, `docs/guide/installation.md`, and generated `docs/public/llms-full.txt`.
- Tests added: exact product/version identity fixtures for empty, wrong-product, and wrong-version output; direct partial asset-download failure; configurable latest-release response with post-cutoff success; preservation and temporary-file cleanup assertions for every failure path.
- Verification: POSIX shell syntax, hermetic installer fixtures, distribution contracts, Cargo fmt/check/test/clippy with locked dependencies, and `bun run docs:build` all pass.
- Discrepancies from design: none.
- Adjacent issues parked: none.
