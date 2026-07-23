---
id: release-1.5.1
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.5.1
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Release 1.5.1

Patch release fixing five rough edges found during a full v1.5.0 MCP-surface
shakedown against live sites and controlled fixtures. Every defect landed with
a design in its item body, a pinned regression, and a green workspace gate.

## Bound items

- `feature-container-text-generic-ancestors` — generic-role ancestors become
  eligible `container_text` scopes under a true-collapsed-length 1024-byte cap
  (allowlisted container roles stay authoritative); `no_match` on a
  container-qualified query now reports uncontained match candidates.
- `feature-semantic-wait-nonactionable` — semantic waits probe the full
  acquired accessibility tree through a count-only registry probe, so
  status/alert/toast content is waitable; `absent` no longer false-positives
  for non-actionable content; `query_page` is unchanged.
- `feature-temporal-partial-retention` — `allow_partial` clamps uniformly
  across every anchor kind; the missing production session-catalog writer now
  persists recording sessions (fixing never-firing
  `requested_end_not_yet_elapsed` and always-failing wall-clock anchors), with
  fail-closed terminal state when the final catalog write fails.
- `feature-popup-click-reconciliation` — side-channel fact assembly polls
  under one bounded 2s ceiling so same-click popups reach `new_pages` facts;
  a fenced window-open watcher caps the completion settle, cutting the
  popup-click worst case from ~6.1s to ~3.75s.
- `story-filmstrip-anchor-default` — omitted artifact anchors default inside
  the visual source sequence (filmstrip: declared source frame time;
  storyboard: first retained frame) instead of failing out-of-range.

## Gate runs

- Designs by fresh-context Opus sub-agents; implementations by cross-model
  gpt-5.6-luna; one cross-model gpt-5.6-sol aggregate review over the full
  change set.
- The review's three material findings (ineffective generic-container cap,
  unfenced reconciliation effects, non-fail-closed terminal catalog write plus
  an unreachable startup sweep) were accepted, fixed, and re-verified in the
  same pass.
- Full workspace gate green after every item: fmt, wire-enum schema check,
  check, tests (72 suites), clippy `-D warnings`.

## Board hygiene

All 215 done items across prior releases were archived to `.work/archive/`
as bodyless ref stubs (`archived_atop: v1.5.1`); full bodies remain in git
history. Active work reduces to the `epic-prove-temporal-advantage` hierarchy
and `feature-perf-scout-adjudication`.
