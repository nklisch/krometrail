---
id: release-1.0.1
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.0.1
gate_origin: null
created: 2026-07-16
updated: 2026-07-16
---

# Release 1.0.1

Short stable patch release for native plugin distribution and exact release-coupled managed binary activation.

## Bound items

- `agent-plugin-distribution`
- `agent-plugin-distribution-canonical-package`
- `agent-plugin-distribution-isolated-qualification`
- `agent-plugin-distribution-marketplace-publication`
- `plugin-managed-binary-bootstrap`
- `plugin-managed-binary-bootstrap-launcher-and-installer`
- `plugin-managed-binary-bootstrap-qualification-and-docs`
- `plugin-managed-binary-bootstrap-release-version-sync`
- `story-fix-release-cross-version-tag`

No archived stubs were unbound. The operator confirmed the complete post-v1.0.0 set.

## Gate runs

- **gate-security** (2026-07-16) — 1 Medium release hardening item deferred to backlog by operator; 1 Low ambient finding already tracked as `gate-security-redact-nested-browser-event-secrets`; no Critical or High findings.
- **gate-tests** (2026-07-16) — 2 release-relevant gaps (1 High, 1 Medium), both fixed and verified: ordinary CI now runs the hermetic bootstrap suite, which covers all supported platform mappings and unsupported-host failures.
- **gate-cruft** (2026-07-16) — no findings after Rust warnings, shell reachability, release-helper, plugin packaging, test-value, comments, compatibility, and validation-layer inspection.
- **gate-docs** (2026-07-16) — no rolling-foundation, README, changelog, plugin-skill, pattern-reference, generated-file, or doc-placement drift.
- **gate-patterns** (2026-07-16) — 2 new patterns codified (`exact-release-managed-activation`, `hermetic-release-boundary-fixtures`); no inconsistencies.

## Changelog

The operator approved the `v1.0.1` changelog entry for shipping on 2026-07-16.


## Shipment

- **Date shipped:** 2026-07-16
- **Mapping:** tag-based
- **Release tag:** `v1.0.1` at `fe409d0a31b3b813bc8eb39336d3fa516bcc0665`
- **GitHub release:** https://github.com/nklisch/krometrail/releases/tag/v1.0.1
- **Release workflow:** https://github.com/nklisch/krometrail/actions/runs/29508478863
- **Sibling catalogs:** https://github.com/nklisch/skills/pull/44 (`5f02257d08e8e355904f140f821691561fb59cdf`)
- **Total items shipped:** 12
- **Gate totals:** security 2 findings (1 Medium operator-deferred, 1 Low already tracked); tests 2 findings fixed; cruft 0; docs 0; patterns 2 codified and 0 inconsistencies.

Published qualification verified all six release files, every checksum, exact Linux x64 `krometrail 1.0.1` identity, GitHub build provenance, native Claude/Codex managed activation, and fresh remote `nklisch/skills` installs at plugin version 1.0.1.

## Shipped items

Bodies live in git history. `git show fe409d0:<former active path>` recovers any pruned body.

| id | title | kind | archived_atop | git ref |
|----|-------|------|---------------|---------|
| `agent-plugin-distribution` | Distribute Krometrail as a native agent plugin | feature | — | `fe409d0` |
| `plugin-managed-binary-bootstrap` | Bootstrap and update the plugin-managed binary | feature | — | `fe409d0` |
| `agent-plugin-distribution-canonical-package` | Build the canonical Claude and Codex plugin package | story | — | `fe409d0` |
| `agent-plugin-distribution-isolated-qualification` | Qualify isolated plugin and binary lifecycles | story | — | `fe409d0` |
| `agent-plugin-distribution-marketplace-publication` | Publish native Krometrail marketplace entries | story | — | `fe409d0` |
| `gate-patterns-1.0.1` | Patterns extracted for 1.0.1 | story | — | `fe409d0` |
| `gate-tests-cover-managed-plugin-platform-matrix` | Cover the managed plugin platform matrix | story | — | `fe409d0` |
| `gate-tests-run-plugin-bootstrap-fixtures-in-ci` | Run managed plugin bootstrap fixtures in ordinary CI | story | — | `fe409d0` |
| `plugin-managed-binary-bootstrap-launcher-and-installer` | Add the plugin-managed launcher and release installer | story | — | `fe409d0` |
| `plugin-managed-binary-bootstrap-qualification-and-docs` | Qualify managed bootstrap and update behavior | story | — | `fe409d0` |
| `plugin-managed-binary-bootstrap-release-version-sync` | Derive plugin versions during product releases | story | — | `fe409d0` |
| `story-fix-release-cross-version-tag` | Pass the tagged cross release to the pinned cross action | story | — | `fe409d0` |
