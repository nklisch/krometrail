---
id: release-1.2.0
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Release 1.2.0

Minor release replacing Krometrail's agent response matrix with one concise-first current contract, removing unsupported compatibility machinery, and repairing capture persistence, batch evidence, and temporal bundle economy.

## Bound items

- `epic-agent-surface-simplification` and its complete five-feature, ten-story hierarchy

The 16-item hierarchy is the complete release scope. No unrelated active or backlog work is included.

## Gate runs

- Separate agile security, test, cruft, documentation, and pattern scans were skipped by operator preference.
- Every feature received one fresh-context standard review; material findings were repaired in the same pass.
- One integrated epic review and one final completion audit covered cross-feature behavior, current-contract coherence, privacy, tests, skill/docs, and compatibility-cruft removal.

## Changelog

The `v1.2.0` entry covers the complete selected minor-release scope, including all review and completion-audit repairs.

## Validation

- Current schema catalog equivalence, exact-current reopen, and incompatible pre-mutation refusal — passed.
- Store artifact/schema integration suites after deleting the obsolete v5 migration fixture — passed.
- Response schema closure, target ranking/bounds, no-inline limits, diagnostics, batch omission, temporal anchor selection/resources, zero default artifact reads, and persistence shutdown recovery regressions — passed.
- Generated public documentation build and workspace all-target compilation — passed.
- Locked workspace formatting, check, all-target tests, and warning-denied Clippy — passed in the release helper and GitHub CI.

## Shipment

- **Date shipped:** 2026-07-18
- **Release tag:** `v1.2.0` at `228af13d865a3e5f7bc76514ffed059325696d32`
- **GitHub release:** https://github.com/nklisch/krometrail/releases/tag/v1.2.0
- **Release workflow:** https://github.com/nklisch/krometrail/actions/runs/29672760698
- **Rust CI:** https://github.com/nklisch/krometrail/actions/runs/29672760628
- **Documentation deployment:** https://github.com/nklisch/krometrail/actions/runs/29672760627
- **Published files:** five platform executables plus `checksums.txt`; all builds, matching-architecture smoke tests, attestations, and publication checks passed.
- **Plugin projection:** native Claude and Codex manifests, catalogs, and the exact managed binary version marker were atomically advanced to 1.2.0 by the release helper.
- **Total bound non-release items shipped:** 16.
