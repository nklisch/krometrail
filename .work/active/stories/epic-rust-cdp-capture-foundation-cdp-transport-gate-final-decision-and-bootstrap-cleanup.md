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

- [x] Decision/report/docs/items agree on exact same-revision evidence and platform-faithful measurements.
- [x] Narrative counts and run URLs derive from authoritative reports and repository identity.
- [ ] Temporary push trigger is removed and manual exact-SHA dispatch remains reproducible; the only remaining action is authorized deletion of remote `origin/ci/cdp-macos-evidence` after this commit.
- [x] Default/spike/candidate quality gates and docs build pass; no production adapter or core-port change lands.

## Implementation notes

- Generated `docs/evidence/cdp-transport/v2/decision.json` solely with `decide_from_files` from the accepted v2 Linux and macOS reports. It selects exact `cdpkit` 0.4.0 and preserves all 13 platform-labelled gate results plus the identical 942-observation candidate-contract trace/results on both platforms.
- Verified accepted report SHA-256 digests: Linux `0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148`; macOS `c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998`. Decision digest: `0288aa9a379b467042409ac27056107b443ea0d91bd21fc4fc8c2beae44c075b`.
- Rolled exact revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2`, configuration/fixture provenance, observed measurements, fallback reasoning, named-event/unbounded-subscriber/RSS-proxy limits, hosted run `29202919716`, corrected owner URLs, and stale 3,553/3,572-era counts through v2 README, research, skill, feature, parent epic, architecture, and story narratives.
- Removed the temporary workflow `push` trigger/comment. Retained strict `workflow_dispatch` ref+SHA verification and resolved-SHA artifact naming. No production adapter or core-port change landed.
- Verification completed locally: report normalization/decisive validation and digest checks, default/spike/cdpkit tests and clippy gates, formatting, and `bun run docs:build`.
- Restored `.work/bin/work-view`; `.pi/` remains ignored.

### Remaining external action

- Delete the authorized remote branch `origin/ci/cdp-macos-evidence` after this exact story commit. Do not perform that deletion locally or push from this story.
