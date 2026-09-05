---
id: epic-a-grade-reliability-response-module-boundaries
kind: feature
stage: backlog
tags: [agent-ux, refactor, testing]
parent: epic-a-grade-reliability
depends_on: [epic-a-grade-reliability-agent-result-delivery, epic-a-grade-reliability-snapshot-freshness]
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Split MCP projection responsibilities around the canonical result

## Outcome and priority

Operation projection, snapshot presentation, evidence resources, and final MCP delivery share a large response module. Clarifying these boundaries should reduce drift between canonical results and what agents see.

- **Priority:** P3 — wave 4 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Maintainability judgment, not a runtime defect.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-mcp/src/response.rs — approximately 3360 lines before its main test module
- crates/krometrail-mcp/src/server.rs — mostly tests, not the same production-size problem

## Acceptance criteria

- [ ] Extract cohesive projection/presentation/delivery modules with one canonical result authority and explicit ownership of omissions/resources/images.
- [ ] Preserve corrected wire and model-visible results, error status, novelty behavior, budgets, and resource authorization byte-for-byte where deterministic.
- [ ] Retain realistic protocol-shaped and integration-delivery tests; do not replace them with assertions against internal structs alone.
- [ ] Avoid needless changes to server.rs merely because its test-heavy total line count is large.

## Implementation direction and boundaries

Refactor after result delivery and novelty fixes. Do not combine observable contract changes with a behavior-preserving cleanup.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
