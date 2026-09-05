---
id: epic-a-grade-reliability-clipboard-error-shape
kind: feature
stage: backlog
tags: [browser, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Classify clipboard failures from the actual CDP result shape

## Outcome and priority

The clipboard classifier searches /result/exceptionDetails even though the unwrapped command result exposes exceptionDetails at its top level. Focus, secure-context, or unavailable-API failures can receive generic permission-denied recovery advice; existing classifier tests use the nested shape.

- **Priority:** P2 — wave 1 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced extractor/test mismatch; permission automation remains a separate issue.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/control/clipboard.rs:252 — nested-only exception extractor
- crates/krometrail-cdp/src/control/evaluation.rs:58 — top-level exception handling
- crates/krometrail-cdp/src/transport/cdpkit.rs:103 — raw result boundary

## Acceptance criteria

- [ ] Use actual transport-shaped fixtures for focus_required, secure_context_required, clipboard_unavailable, permission denial, malformed results, and success.
- [ ] Each known failure reports its accurate category and actionable recovery; unknown exceptions do not become a confident permission diagnosis.
- [ ] Normalize the supported transport envelope consistently without proliferating legacy result variants.
- [ ] Keep raw clipboard content and sensitive exception material out of diagnostic summaries.

## Implementation direction and boundaries

Correct result normalization and classification before assuming that every observed clipboard failure requires a permission grant.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-browser-automated-clipboard-permissions` — related authority/context, not an implicit blocking dependency.
