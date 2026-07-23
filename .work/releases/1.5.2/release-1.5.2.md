---
id: release-1.5.2
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.5.2
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Release 1.5.2

Patch release restoring `query_page` and semantic-wait availability on large
real-world documents, found during the v1.5.1 live shakedown.

## Bound items

- `feature-query-node-limit-large-pages` — the snapshot acquisition bounds
  rise from an arbitrary 5,000 nodes / 1 MiB text to 50,000 nodes / 8 MiB
  text, restoring ergonomic queries and semantic waits on heavy pages
  (large Wikipedia articles, documentation sites, dense dashboards) while
  still fail-closing on pathological trees. The refusal's recovery guidance
  now names actions that work for both queries and waits, and the dead
  geometry-recovery branch behind a mislabeled query error is removed.
  Three quietly-quadratic paths that were only affordable at the old bound
  (parent-depth validation, DOM subtree text aggregation, container-ancestor
  lookup) are now linear, pinned by a deep-chain tripwire and an 8k-node
  DOM `container_text` regression.

## Gate runs

- Design by a fresh-context Opus sub-agent; implementation and review fixes
  by cross-model gpt-5.6-luna; one cross-model gpt-5.6-sol review pass.
- The review's five material findings (three superlinear paths, an
  understated memory envelope, wait-unusable recovery text) were accepted,
  fixed, and re-verified in the same pass.
- Full workspace gate green: fmt, wire-enum schema check, check, tests
  (74 suites, 1,291 tests), clippy `-D warnings`.
