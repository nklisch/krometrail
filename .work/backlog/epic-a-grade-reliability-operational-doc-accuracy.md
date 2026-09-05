---
id: epic-a-grade-reliability-operational-doc-accuracy
kind: feature
stage: backlog
tags: [prose, infra, agent-ux]
parent: epic-a-grade-reliability
depends_on: [epic-a-grade-reliability-minimum-rust-gate, epic-a-grade-reliability-release-version-ownership, epic-a-grade-reliability-doctor-discovery-only, epic-a-grade-reliability-agent-result-delivery, epic-a-grade-reliability-page-selection-recovery]
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Align operational documentation with verified executable behavior

## Outcome and priority

Documentation must accurately describe compiler requirements, discovery side effects, release ownership, profile/page recovery, supported integrations, and model-visible results. Duplicated agent instructions also create maintenance noise.

- **Priority:** P3 — wave 4 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Documentation drift and duplicated instructions observed; runtime claims must follow completed fixes.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- docs/guide/troubleshooting.md — doctor and recovery guidance
- docs/reference/configuration.md — actual runtime controls
- docs/RELEASING.md — version ownership
- AGENTS.md and .agents/AGENTS.md — repeated independent-release paragraph
- docs/public/llms-full.txt — generated, never hand-edited

## Acceptance criteria

- [ ] Trace revised operational claims and command examples to the current executable/schema and completed fixes; distinguish intended foundation direction from implemented capabilities.
- [ ] Keep troubleshooting usable from an agent's observed symptom, including stale profile, missing selection, and success-only result delivery; do not instruct indiscriminate data-directory deletion.
- [ ] Remove duplicated independent-release instructions in both current instruction entry points without establishing a new competing source of truth.
- [ ] Update affected plugin skills/reference pages and regenerate generated documentation through bun run docs:build, never by hand.
- [ ] Review local links/examples and publication wording; do not claim uncollected platform/model evidence or imply future capabilities already exist.

## Implementation direction and boundaries

This item is for final reconciliation, not permission to defer essential contract docs out of their owning fixes.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-profile-lock-fallback-or-recovery` — related authority/context, not an implicit blocking dependency.
