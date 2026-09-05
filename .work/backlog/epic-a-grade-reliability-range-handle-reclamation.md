---
id: epic-a-grade-reliability-range-handle-reclamation
kind: feature
stage: backlog
tags: [storage, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Reclaim resolved-range handle entries and byte budget

## Outcome and priority

Invalidated/deleted source evidence leaves its handle entry and budget charge in the process map. New distinct valid ranges eventually fail even when old handles cannot be used.

- **Priority:** P2 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced: entries and budget are never removed/released.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- src/range_handles.rs:55 — register and resolve_available
- crates/krometrail-core/src/range_handle.rs — 4096-entry and 16 MiB contract limits

## Acceptance criteria

- [ ] Define a bounded handle lifetime/reclamation policy and expose actionable expiry/invalidation recovery without promising indefinitely stable process handles.
- [ ] Invalidated/deleted entries release both entry and byte capacity. Test with small injected limits and a long-session scenario beyond the default historical admission count.
- [ ] Live handle dereferences continue revalidating session/target scope, exact source identities, ordering, and retained availability.
- [ ] Deduplication, concurrent register/resolve/reclaim, accounting overflow, and full-capacity errors remain deterministic; successful evidence generation does not become an unrecoverable capacity dead end.

## Implementation direction and boundaries

Choose expiry/eviction and authority-driven invalidation at design time. Reclaim dead entries and define behavior for live entries; increasing limits is not a fix.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.
