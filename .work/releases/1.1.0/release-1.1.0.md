---
id: release-1.1.0
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Release 1.1.0

Stable minor release making Krometrail's ordinary agent surface compact and ergonomic by default while preserving explicit evidence-rich expansion and adding bounded browser-context and local-I/O workflows.

## Bound items

- `epic-agent-browser-ergonomics` and its complete six-feature, fifteen-story hierarchy
- `story-fix-lazy-managed-download-activation`
- `story-fix-bound-page-target-state`
- `story-fix-profile-inventory-canonical-path-test`

The original 25-item bundle plus 23 quality-gate findings/tracking items are bound, for 48 non-release items total. The operator explicitly included the gates' ambient documentation drift and low-severity security hardening rather than deferring them. No older unrelated active or backlog work is included.

## Gate runs

- **gate-security** — 3 findings (Medium 1, Low 2); all repaired and `done`, including isolated clipboard execution, cancellation evidence, and stale-download scavenging.
- **gate-tests** — 4 gaps (High 3, Medium 1); all repaired and `done`, including real-Chrome clipboard/context qualification and successful MCP mutation projection round-trips.
- **gate-cruft** — 5 Medium findings; all simplified and `done` with no behavior or guarantee reduction.
- **gate-docs** — 10 high-confidence drift findings; all repaired and `done`, with generated docs and catalog-wide pattern anchors verified.
- **gate-patterns** — 4 structural patterns extracted, indexed, and published in the hook-loaded digest; no inconsistencies.

## Changelog

The `v1.1.0` changelog entry covers the complete selected minor-release scope and its repaired aggregate-review blockers.

## Validation

- Feature-level real Chrome semantic, viewport, and managed-download qualifications — passed.
- Fresh standard review for all six child features and one aggregate epic review — passed after accepted findings were repaired.
- Generated public docs and root runtime smoke — passed.
- Full locked workspace all-target tests, check, warning-denied Clippy, and formatting — passed after gate repairs.
- Real-Chrome browser-context qualification — passed frame query/click, root scroll, stale-reference fencing, 256-asset bound, omission count, and privacy assertions.
- Real-Chrome clipboard qualification — passed the declared host permission/timeout denial path with recovery and no sentinel leakage.
- Locked workspace formatting, check, all-target tests, and Clippy — passed in the release helper and GitHub CI, including Rust 1.85 MSRV.

## Shipment

- **Date shipped:** 2026-07-18
- **Release tag:** `v1.1.0` at `7d32a0df3918f8cacab0ab16c6886efcac651ea5`
- **GitHub release:** https://github.com/nklisch/krometrail/releases/tag/v1.1.0
- **Release workflow:** https://github.com/nklisch/krometrail/actions/runs/29660199652
- **Rust CI:** https://github.com/nklisch/krometrail/actions/runs/29660199661
- **Documentation deployment:** https://github.com/nklisch/krometrail/actions/runs/29660199621
- **Published files:** five platform executables plus `checksums.txt`; all builds, matching-architecture smoke tests, attestations, and publication checks passed.
- **Plugin projection:** native Claude and Codex manifests, catalogs, and the exact managed binary version marker were atomically advanced to 1.1.0 by the release helper.
- **Total bound non-release items shipped:** 48.
