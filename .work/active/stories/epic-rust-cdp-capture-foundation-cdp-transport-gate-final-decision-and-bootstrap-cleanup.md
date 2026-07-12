---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-final-decision-and-bootstrap-cleanup
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-cross-platform-requalification]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Regenerate the transport decision and remove temporary bootstrap paths

## Origin

Phase 2 feature review found that the decision exposes Linux-only measurements, narrative counts drifted from macOS evidence, and the temporary push-triggered evidence bootstrap remains live.

## Scope

Regenerate the schema-v2 decision solely from repaired same-revision reports, preserving each platform's labelled gates and candidate-contract trace/results. Roll exact measurements, digests, revision, selection, and limitations through evidence README, research, skill, feature, parent epic, architecture, and story narratives. Remove the temporary push trigger and delete the authorized remote bootstrap branch after hosted evidence is safely committed; retain exact-ref/SHA manual dispatch only and use resolved SHA in artifact names.

## Acceptance criteria

- [ ] Decision/report/docs/items agree on exact same-revision evidence and platform-faithful measurements.
- [ ] Narrative counts and run URLs derive from authoritative reports and repository identity.
- [ ] Temporary push trigger and remote `ci/cdp-macos-evidence` branch are removed after evidence lands; manual exact-SHA dispatch remains reproducible.
- [ ] Default/spike/candidate quality gates and docs build pass; no production adapter or core-port change lands.
