---
id: epic-a-grade-reliability-storage-responsiveness
kind: feature
stage: backlog
tags: [storage, perf, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Measure and bound storage interference with browser responsiveness

## Outcome and priority

Slow SQLite transactions, disk operations, or checkpoint contention can block the runtime thread servicing browser events, timers, and cancellation. The review did not measure a production stall or select a replacement architecture.

- **Priority:** P2 — wave 3 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Risk: synchronous work on the single-threaded runtime is established; production latency impact has not been measured.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- src/main.rs:36 — new_current_thread runtime
- crates/krometrail-store/src/recording.rs:2210 — synchronous SQLite work inside append_frame
- crates/krometrail-store/src/index/maintenance.rs:89 — blocking checkpoint retries
- src/app.rs:419 — configured SQLite busy timeout

## Acceptance criteria

- [ ] Establish release-build baseline and contention scenarios with browser-command latency, event-loop delay, screencast acknowledgement/ingestion, loss accounting, cancellation latency, and storage throughput.
- [ ] Agree and record quantitative latency/cancellation budgets before implementing a change, using docs/EVALUATION.md where applicable.
- [ ] If interference exceeds the declared budget, isolate blocking persistence behind bounded scheduling while preserving single-writer ordering, durability, admission pressure, and explicit gaps.
- [ ] Fault/slow-storage tests prove deadlines and recovery behavior. If the hypothesis is falsified, retain measured evidence and regression coverage rather than forcing an unnecessary rewrite.

## Implementation direction and boundaries

Consider a bounded dedicated storage worker versus localized blocking work only after tracing transaction/lock ownership. Merely switching to a multithreaded runtime is not proof that the problem is solved.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `perf-scout-profile-artifact-stages` — related authority/context, not an implicit blocking dependency.
