---
id: refactor-centralize-cargo-toml-section-parsing
kind: story
stage: implementing
tags: [refactor, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-12
updated: 2026-07-12
---

# Centralize Cargo TOML section parsing in the release helper

## Brief

`scripts/bump-version.ts:18-26`, `scripts/bump-version.ts:71-76`, `scripts/bump-version.ts:86-91`, and `scripts/bump-version.ts:95-99` independently calculate the start, content boundary, and end of a TOML section. The four copies already vary in missing-section handling, which makes the release helper's intentionally narrow Cargo parser harder to audit.

Extract one local helper that resolves an exact TOML section header and returns its bounds/content, with explicit required-versus-optional handling at each call site. Use it for the root package, workspace package metadata, workspace member list, and member package sections. Keep the current narrow parser and every error/output/write behavior intact; adopting a general TOML library or broadening accepted manifest syntax is out of scope.

**Source lens**: missing abstraction / code smell

**Rationale**: removes four copies of release-critical boundary arithmetic and gives the helper one auditable definition of where a Cargo TOML section ends.

**Black-box classification**: pure refactor. Identical manifests, arguments, malformed inputs, files, console output, Cargo commands, rollback behavior, commits, tags, and pushes must produce identical outcomes.

## Acceptance criteria

- [ ] `scripts/bump-version.ts` has one section-boundary helper used for `[package]`, `[workspace.package]`, `[workspace]`, and workspace-member `[package]` lookup.
- [ ] Existing required and optional section semantics and exact release side effects remain unchanged.
- [ ] `bash tests/distribution-static.sh` passes, including prepare, dry-run, lock-refresh, rollback, and duplicate-package fixtures.
- [ ] `bun scripts/bump-version.ts patch --dry-run` succeeds without changing tracked files.

## Risk and rollback

**Risk**: Medium. This is release-critical parsing, and an off-by-one boundary error could target the wrong version assignment even though the intended behavior is unchanged.

**Rollback**: Revert the implementation commit to restore the inline section calculations.

## Discovery notes

- Scope: second mandatory five-story autopilot cadence; distribution workflows/scripts/manifests/static contract tests, current contributor/docs navigation surfaces, and remediation-touched core invariant/enum modules.
- Dispatch: direct-read only as required; no questions or subagents. `.pi/`, escalated review metadata, generated lockfile internals, and the existing `refactor-derive-cli-error-code-names` finding were excluded.
- Value: medium — a small local abstraction removes repeated release-critical parsing without introducing another parser dependency.
