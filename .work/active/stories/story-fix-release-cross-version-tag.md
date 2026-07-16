---
id: story-fix-release-cross-version-tag
kind: story
stage: review
tags: [bug, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Pass the tagged cross release to the pinned cross action

## Symptom

The v1.0.0 release workflow failed only for `krometrail-linux-arm64`; `houseabsolute/actions-rust-cross` ran `cargo install cross --git ... --rev 0.2.5`, and Git reported `revspec '0.2.5' not found`. Publication was skipped.

## Root cause

The action's `cross-version` input is used as a Git revision when installing cross. The repository tag is `v0.2.5`, but the workflow and its static test incorrectly required `0.2.5`.

## Fix approach

Pass the exact upstream tag `v0.2.5` and update the release static contract. Preserve the pinned action SHA, cross images, targets, smoke tests, and immutable release tag.

## Regression test

`tests/distribution-static.sh` requires `cross-version: v0.2.5`, preventing the invalid unprefixed revision from returning.

## Implementation notes

- **Execution capability:** inline focused fix; two one-line contract changes with direct CI reproduction.
- **Files changed:** `.github/workflows/release.yml`, `tests/distribution-static.sh`.
- **Confirmation:** upstream `refs/tags/v0.2.5` resolves to `f8151ae777290430cf2108efacf3976d9528500b`; unprefixed `refs/tags/0.2.5` is absent; distribution/installer/version-helper contract suite passes; diff check passes.
- **Original reproduction:** GitHub Actions run `29469495815`, Linux arm64 job, failed with `revspec '0.2.5' not found` before any build.
- **Adjacent issues parked:** none.
