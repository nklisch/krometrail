---
id: release-1.0.4
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.0.4
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Release 1.0.4

Stable patch release for truthful dynamic viewport evidence, resilient compact temporal bundles, and bounded live observation feedback.

## Bound items

- `truthful-screencast-geometry`
- `resilient-compact-temporal-bundles`
- `resilient-compact-temporal-bundles-guide-captured-bounds`
- `resilient-compact-temporal-bundles-fit-high-dpi`
- `resilient-compact-temporal-bundles-project-manifests`
- `compact-live-observations`
- `compact-live-observations-bound-snapshots`
- `compact-live-observations-deduplicate-warnings`

The operator selected the complete manual-test finding set for the next patch. No older unbound work is included.

## Gate runs

- **gate-security** — skipped by operator for this focused patch.
- **gate-tests** — skipped by operator; focused feature reviews and regression-driven verification remain required.
- **gate-cruft** — skipped by operator for this focused patch.
- **gate-docs** — skipped by operator; changelog and shipped skill guidance were updated in the feature bundle.
- **gate-patterns** — skipped by operator for this focused patch.

## Changelog

The `v1.0.4` changelog entry covers the complete selected patch scope.

## Validation

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- Real Chrome viewport apply/navigation/clear/target-isolation qualification — passed.
- Real headless capture produced and validated an initial frame with source-safe fidelity metadata. The legacy 30-frame ambient-compositor cadence smoke timed out identically at the released `v1.0.3` tag on this host, so it is recorded as an environment/Chrome limitation rather than a 1.0.4 regression.

## Shipment

- **Date shipped:** 2026-07-17
- **Release tag:** `v1.0.4` at `620b5a51249c75b73e6331e0c04461e52ab81978`
- **GitHub release:** https://github.com/nklisch/krometrail/releases/tag/v1.0.4
- **Release workflow:** https://github.com/nklisch/krometrail/actions/runs/29628634212
- **Rust CI:** https://github.com/nklisch/krometrail/actions/runs/29628633853
- **Documentation deployment:** https://github.com/nklisch/krometrail/actions/runs/29628633863
- **Published files:** five platform executables plus `checksums.txt`; every build, architecture smoke test, attestation, and publication step passed.
- **Plugin qualification:** isolated native Claude and Codex installation, exact v1.0.4 managed bootstrap, MCP tool discovery, and all three evidence resource templates passed.
- **Total items shipped:** 8.
