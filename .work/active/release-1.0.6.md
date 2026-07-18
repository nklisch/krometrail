---
id: release-1.0.6
kind: release
stage: release-ready
tags: []
parent: null
depends_on: []
release_binding: 1.0.6
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Release 1.0.6

Stable patch release for focus-preserving visible-browser control and the complete finding set from
the latest manual public-site test round.

## Bound items

- `feature-preserve-browser-focus`
- `story-fix-batch-direct-target-inheritance`
- `story-fix-batch-schema-rendering`
- `story-compact-batch-step-results`
- `story-fix-navigation-viewport-observation-race`

No older unbound backlog work is included.

## Gate runs

- **gate-security** — skipped by operator for this focused patch.
- **gate-tests** — skipped by operator; focused regression tests, item reviews, workspace release
  verification, and real-browser qualification remain required.
- **gate-cruft** — skipped by operator for this focused patch.
- **gate-docs** — skipped by operator; foundation docs, changelog, and shipped skill guidance are
  updated in this bundle.
- **gate-patterns** — skipped by operator for this focused patch.

## Changelog

The `v1.0.6` changelog entry covers the complete selected patch scope.

## Validation

- Focused core batch-target regression — passed.
- Focused generated MCP batch-schema regression — passed.
- Focused compact batch-response regression — passed.
- Focused navigation viewport/evidence ordering regression — passed.
- Focus-policy core, MCP, CDP, pointer, and lifecycle regressions — passed.
- Opt-in real Chrome preserve-focus qualification — passed; the original page remained visible while
  `create_page` produced a hidden background tab without activation.
- Locked workspace formatting, check, all-target tests, and Clippy — pending release helper.
