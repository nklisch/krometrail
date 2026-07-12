---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-integrity
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

- [ ] A pre-existing release binary cannot bypass a current developer build.
- [ ] Mismatched or malformed release tags fail before artifact build or publication.
- [ ] Documentation dependencies install reproducibly from a committed lockfile.
- [ ] CI proves both MSRV 1.85 compatibility and current-stable quality.
- [ ] Version bumps perform a narrow lock refresh followed by locked gates.
- [ ] Static distribution tests cover every corrected failure path and the full Rust/distribution gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review; it also absorbs both GLM 5.2 Phase 1 nits after independent verification.
