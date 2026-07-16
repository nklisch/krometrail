---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-final-decision-and-bootstrap-cleanup
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-cross-platform-requalification]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Regenerate the transport decision and remove temporary bootstrap paths

## Origin

Phase 2 feature review found that the prior-v2 decision exposed Linux-only measurements, narrative counts drifted from macOS evidence, and the temporary push-triggered evidence bootstrap remained live. This completed story records the prior-v2 cleanup milestone; the final v3 rollup supersedes its evidence claims.

## Scope

Regenerate the schema-v2 decision solely from repaired same-revision reports, preserving each platform's labelled gates and candidate-contract trace/results. Roll exact measurements, digests, revision, selection, and limitations through evidence README, research, skill, feature, parent epic, architecture, and story narratives. Remove the temporary push trigger and delete the authorized remote bootstrap branch after hosted evidence is safely committed; retain exact-ref/SHA manual dispatch only and use resolved SHA in artifact names.

## Acceptance criteria

- [x] Decision/report/docs/items agree on exact same-revision evidence and platform-faithful measurements.
- [x] Narrative counts and run URLs derive from authoritative reports and repository identity.
- [x] Temporary push trigger and remote bootstrap branch are removed; manual exact-SHA dispatch remains reproducible.
- [x] Default/spike/candidate quality gates and docs build pass; no production adapter or core-port change lands.

## Implementation notes

- Prior-v2 milestone generated `docs/evidence/cdp-transport/v2/decision.json` solely with `decide_from_files`; its reports and decision are retained byte-for-byte under `docs/evidence/cdp-transport/v2/historical/` and are not current inputs.
- The final v3 rollup regenerated the superseded canonical decision from reports at exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`; those bytes are preserved under `docs/evidence/cdp-transport/v2/historical/final-v2-07b0990/`. Current final5 report/decision digests and measurements are recorded by `epic-rust-cdp-capture-foundation-cdp-transport-gate-attested-final-recapture`.
- The cdp-transport workflow has no push trigger. Strict manual `workflow_dispatch` ref+SHA verification and resolved-SHA artifact naming remain. No production adapter or core-port change landed.
- Verification completed locally: report normalization/decisive validation and digest checks, default/spike/cdpkit tests and clippy gates, formatting, and `bun run docs:build`.
- Restored `.work/bin/work-view`; `.pi/` remains ignored.

### External cleanup status

- The temporary remote branch `origin/ci/cdp-macos-evidence` is intentionally not deleted in this local story; the parent story owns that authorized external action.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** This prior-v2 milestone review verified its historical artifacts. The final v3 rollup is the current source of truth and separately tracks authorized remote branch cleanup.
