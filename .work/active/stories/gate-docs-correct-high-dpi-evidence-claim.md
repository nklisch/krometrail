---
id: gate-docs-correct-high-dpi-evidence-claim
kind: story
stage: review
tags: [documentation, testing, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: docs
created: 2026-07-15
updated: 2026-07-15
---

# Correct the cross-platform high-DPI evidence claim

## Drift category
Contradictory qualification documentation

## Location
- Doc: `docs/evidence/cross-platform-smoke/v1/README.md:33-34,119-122`
- Contradicting evidence: absent `macos-chrome-high-dpi.json` and the README's recorded failed `>= 1.5` observation

## Current doc text

> Default-DPI and high-DPI macOS configurations are both exercised; both force device scale.

## Contradiction

A high-DPI attempt ran, but production metadata observed scale one, the decisive assertion failed, and no passing high-DPI artifact exists.

## Required edit

Describe attempted execution separately from passing evidence, retain the explicit absent-artifact/non-claim boundary, and regenerate public documentation with `bun run docs:build`. Do not weaken the high-DPI threshold or block the local-tool release on absent evidence.

## Implementation notes
- Execution capability: inline direct-read documentation wave; evidence wording was reconciled with the committed artifact set and the production high-DPI assertion behavior.
- Review weight: standard; caller explicitly requested all standalone stories remain at `stage: review` for independent bounded review.
- Files changed: `docs/evidence/cross-platform-smoke/v1/README.md` and generated `docs/public/llms-full.txt`.
- Tests added/removed: none; the existing smoke contract already enforces observed high-DPI scale and absent-artifact semantics.
- Simplification: removed the claim that high-DPI passed and stated attempted execution, failed `>= 1.5` observation, absent artifact, and release non-blocking status once each.
- Discrepancies from design: none.
- Verification evidence: `bun run docs:build` passed (including VitePress link/build checks); `cargo test -p krometrail-cdp --test cross_platform_smoke --locked` passed all 13 schema/canonical checks, and the workspace Rust gates passed under Rust 1.95.0 (project MSRV 1.85).
- Adjacent issues parked: none.
