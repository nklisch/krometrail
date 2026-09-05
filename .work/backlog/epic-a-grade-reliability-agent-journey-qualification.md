---
id: epic-a-grade-reliability-agent-journey-qualification
kind: feature
stage: backlog
tags: [agent-ux, testing, browser]
parent: epic-a-grade-reliability
depends_on: [epic-a-grade-reliability-agent-result-delivery, epic-a-grade-reliability-snapshot-freshness, epic-a-grade-reliability-page-selection-recovery]
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Qualify task completion through the actual agent integrations

## Outcome and priority

An operational browser is insufficient when an agent cannot see results, form the next request, or recover. Repeated local agent friction must be measured at task-completion level, not inferred from tools returning success.

- **Priority:** P2 — wave 3 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** User-reported workflow failure plus a missing end-to-end assurance boundary; not a proven page-selection root cause.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- Review incident: browser launched, corrected screenshot could not find selected page, list_pages exposed success without IDs, agent fell back to desktop control
- crates/krometrail-mcp/src/response.rs — server presentation boundary
- plugin/ — shipped integration manifests and launchers
- docs/EVALUATION.md — browser-control and named-model evaluation discipline

## Acceptance criteria

- [ ] Create a reproducible local fixture journey: start/attach → list → select → screenshot → mutate → inspect changed state → retrieve temporal evidence → recover after target closure → stop.
- [ ] Exercise each currently supported agent integration and capture sanitized model-visible result shape, required IDs, argument corrections, tool-call count, task success, and fallback-to-desktop incidence.
- [ ] Include the reported incident without retaining private site content. Attribute missing selection and lost results separately using correlated server/client traces.
- [ ] Scripted mandatory journeys pass without hidden IDs, fabricated selection, or undocumented desktop rescue; observational agent runs report actual completion rate and recovery cost rather than implying success from scripted transport tests.
- [ ] Use existing >=95% browser-control completion and other relevant evaluation criteria for the declared workload; pin sample sizes and protocol before measurement. Paid/model-specific runs require separate explicit authorization and budget.
- [ ] Reuse existing temporal-advantage evaluation for concept claims; completing this smoke journey alone does not prove temporal reasoning benefit.

## Implementation direction and boundaries

Build the smallest actual-integration harness rather than another mock result consumer. Close defects here through their owning items, not duplicate implementations.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-mcp-locator-ergonomics` — related authority/context, not an implicit blocking dependency.
- `idea-mcp-scroll-delta-simplification` — related authority/context, not an implicit blocking dependency.
- `idea-temporal-range-active-target-defaults` — related authority/context, not an implicit blocking dependency.
- `epic-prove-temporal-advantage-agent-debugging-qualification` — related authority/context, not an implicit blocking dependency.
