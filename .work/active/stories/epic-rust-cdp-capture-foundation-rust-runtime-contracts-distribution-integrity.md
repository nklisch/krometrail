---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-integrity
kind: story
stage: done
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Harden distribution and toolchain integrity

## Scope

Resolve the deep-review distribution findings as one cohesive release-boundary correction. Ensure developer installs always build current source, release tags match Cargo's version, retained Bun documentation tooling is locked, the declared Rust 1.85 minimum is tested, release-helper Cargo checks are locked after an explicit version lock refresh, and deleted harness ignore rules are removed.

## Requirements

- Always run `cargo build --locked --release` before developer installation; extend tests to reject stale-binary reuse.
- Validate strict `v<semver>` release tags against root Cargo package version before every build/publication path, including manual dispatch.
- Track the root Bun lockfile and use frozen installs for documentation CI.
- Add an MSRV CI job pinned to Rust 1.85 while retaining the stable quality gate.
- After version edits, explicitly refresh only expected workspace lock metadata, verify the lockfile delta, then run Cargo check/test/clippy with `--locked`.
- Remove obsolete agent-harness `.gitignore` entries.

## Acceptance criteria

- [x] A pre-existing release binary cannot bypass a current developer build.
- [x] Mismatched or malformed release tags fail before artifact build or publication.
- [x] Documentation dependencies install reproducibly from a committed lockfile.
- [x] CI proves both MSRV 1.85 compatibility and current-stable quality.
- [x] Version bumps perform a narrow lock refresh followed by locked gates.
- [x] Static distribution tests cover every corrected failure path and the full Rust/distribution gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review; it also absorbs both GLM 5.2 Phase 1 nits after independent verification.

## Implementation notes

- Files changed: `.github/workflows/ci.yml`, `.github/workflows/deploy-pages.yml`, `.github/workflows/release.yml`, `.gitignore`, `bun.lock`, `docs/guide/development.md`, `docs/public/llms-full.txt` (regenerated), `scripts/bump-version.ts`, `scripts/dev-install.sh`, `scripts/validate-release-tag.sh`, and `tests/distribution-static.sh`.
- Tests added: isolated release-tag mismatch/malformed-tag fixtures; stale developer-binary replacement fixture with a fake Cargo builder; committed-lock and frozen-docs assertions; MSRV and locked-gate workflow assertions; narrow lock-refresh fixture.
- Verification: `cargo fmt --all -- --check`, locked workspace check/test/clippy, distribution static contracts, shell syntax checks, `bun install --frozen-lockfile`, and `bun run docs:build` all passed. Root `bump-version.ts patch --prepare` also passed all locked gates and was restored without creating a tag, commit, or push.
- Discrepancies from design: none. No core Rust domain files were touched.
- Dispatch: direct local reads and implementation only, per caller instruction; no questions or subagents.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane review follow-up. The orchestrator independently reran formatting, all 37 workspace tests, locked clippy, and the full distribution failure-path suite successfully, and spot-checked the MSRV, always-build install, and tag-validation wiring. The distribution findings and both Phase 1 nits are addressed. Verdict: Approve - story verified by implement; fast-lane advance.
