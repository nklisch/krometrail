---
id: release-1.0.7
kind: release
stage: release-ready
tags: []
parent: null
depends_on: []
release_binding: 1.0.7
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Release 1.0.7

Stable patch release restoring continuous capture after an isolated frame-event subscription loss.

## Bound items

- `story-fix-capture-stream-reconnect`

No older unbound backlog work is included.

## Gate runs

- **gate-security** — skipped by operator for this focused patch.
- **gate-tests** — skipped by operator; focused red/green regression, full workspace tests, and a
  real-Chrome reconnect qualification were completed.
- **gate-cruft** — skipped by operator for this focused patch.
- **gate-docs** — skipped by operator; the existing reconnect contract remains accurate and the
  changelog records the repaired behavior.
- **gate-patterns** — skipped by operator for this focused patch.

## Changelog

The `v1.0.7` changelog entry covers the complete selected patch scope.

## Validation

- Isolated frame-event-stream closure regression — passed; capture restored on attachment
  generation 2.
- Krometrail CDP adapter suite — passed.
- Opt-in real Chrome disconnect/reconnect qualification — passed with 20 frames before disconnect
  and 8 frames on the replacement generation.
- Locked full workspace all-target tests — passed.
- Locked workspace formatting, check, all-target tests, and Clippy — pending release helper.

