---
id: gate-tests-cover-managed-plugin-platform-matrix
kind: story
stage: done
tags: [testing, distribution]
parent: null
depends_on: [gate-tests-run-plugin-bootstrap-fixtures-in-ci]
release_binding: 1.0.1
gate_origin: tests
created: 2026-07-16
updated: 2026-07-16
---

# Cover the managed plugin platform matrix

## Priority

Medium

## Value evidence

Item: `plugin-managed-binary-bootstrap-launcher-and-installer`

Automatic bootstrap promises Linux and macOS on x64 and arm64, but the hermetic test only exercised Linux x64 asset selection.

## Gap type

Valuable platform-partition coverage for a first-activation release boundary.

## Implementation

Parameterize the fake host and release assets. Exercise successful Linux/macOS x64/arm64 selection and explicit unsupported OS/architecture failures with no stdout or managed publication.

## Acceptance evidence

- All four supported host/architecture mappings select the exact stable asset.
- Unsupported OS and architecture fail explicitly before publication.
- Matrix checks remain hermetic and run in ordinary CI through the bootstrap suite.

## Outcome

The fixture now serves all four release assets and parameterizes `uname` so Linux/macOS x64/arm64 each prove exact asset selection and publication. FreeBSD and riscv64 prove explicit no-stdout, no-publication failure. The complete distribution contract passes.
