---
id: gate-tests-run-plugin-bootstrap-fixtures-in-ci
kind: story
stage: done
tags: [testing, distribution]
parent: null
depends_on: []
release_binding: 1.0.1
gate_origin: tests
created: 2026-07-16
updated: 2026-07-16
---

# Run managed plugin bootstrap fixtures in ordinary CI

## Priority

High

## Value evidence

Item: `plugin-managed-binary-bootstrap-qualification-and-docs`

The hermetic fixture protects cold install, warm offline startup, version transitions, concurrent publication, failure preservation, unsafe paths, and MCP stdout purity, but it was only run manually.

## Gap type

Important release seam: a high-value existing regression suite was not wired into the ordinary distribution contract.

## Implementation

Invoke `tests/plugin-bootstrap-fixtures.sh` from `tests/distribution-static.sh`, which already runs in Rust CI and release preparation. Add explicit shell syntax validation in `.github/workflows/ci.yml`. Keep the network/native `plugin-install-smoke.sh` opt-in.

## Acceptance evidence

- Ordinary `tests/distribution-static.sh` executes the hermetic bootstrap suite.
- CI syntax-checks the suite.
- The full distribution contract passes.

## Outcome

The hermetic bootstrap fixture now runs through `tests/distribution-static.sh` in every ordinary Rust CI/release preparation gate, and CI syntax-checks it explicitly. The complete distribution contract passes.
