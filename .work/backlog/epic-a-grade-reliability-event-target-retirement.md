---
id: epic-a-grade-reliability-event-target-retirement
kind: feature
stage: backlog
tags: [browser, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Release browser-event capacity when page targets retire

## Outcome and priority

The domain removes retired targets but the event pipeline retains registrations until shutdown. Distinct closed pages continue to consume the active-target allowance and can prevent later event collection.

- **Priority:** P2 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced lifecycle leak; browser-facing churn regression not yet executed.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- crates/krometrail-cdp/src/events/domain.rs:547 — retire_target closes ingress without pipeline removal
- crates/krometrail-cdp/src/events/pipeline.rs:150,260 — admission and removal
- crates/krometrail-cdp/src/events/mod.rs:54 — default active target cap 32

## Acceptance criteria

- [ ] Repeatedly open and close at least 64 distinct targets with live concurrency below 32; later targets still collect and persist events.
- [ ] A genuinely excessive concurrent live-target count still receives the configured explicit limit outcome.
- [ ] Retirement drains or explicitly accounts for queued events/gaps, terminates writers within a bounded deadline, and releases registrations/capacity.
- [ ] Cover generation replacement, reconnect, duplicate retirement, sink failure, and shutdown races without deleting a replacement target's pipeline.

## Implementation direction and boundaries

Give retired pipelines an explicit drain-and-remove lifecycle; do not raise the cap to hide leaked registrations.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
