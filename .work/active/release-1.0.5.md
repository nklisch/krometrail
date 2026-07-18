---
id: release-1.0.5
kind: release
stage: release-ready
tags: []
parent: null
depends_on: []
release_binding: 1.0.5
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Release 1.0.5

Stable patch release for resilient navigation capture and clearer, target-correct browser-agent control feedback.

## Bound items

- `story-fix-navigation-geometry-refresh`
- `story-fix-pointer-activation-visibility`
- `story-fix-target-local-capture-warnings`
- `story-fix-batch-step-schema`

The operator selected the complete finding set from the latest public-site manual test round. No older unbound work is included.

## Gate runs

- **gate-security** — skipped by operator for this focused patch.
- **gate-tests** — skipped by operator; focused regression tests, standalone reviews, workspace verification, and real-browser qualification remain required.
- **gate-cruft** — skipped by operator for this focused patch.
- **gate-docs** — skipped by operator; the changelog and shipped skill guidance are updated in this bundle.
- **gate-patterns** — skipped by operator for this focused patch.

## Changelog

The `v1.0.5` changelog entry covers the complete selected patch scope.

## Validation

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- Real Chrome viewport apply/navigation/clear/target-isolation qualification — passed.
- Local-candidate MCP schema discovery exposed 19 concrete batch branches with `operation` and `request` objects.
- Local-candidate MCP public-site qualification applied a 360x640 mobile viewport, navigated from MDN to Wikipedia, returned no warnings, and retained healthy capture with 23 persisted frames.
