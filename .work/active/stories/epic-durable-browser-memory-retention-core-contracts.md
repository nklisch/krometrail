---
id: epic-durable-browser-memory-retention-core-contracts
kind: story
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Core Retention Contracts and Budget-Aware Capture Vocabulary

## Checkpoint

Publish the infrastructure-free retention contract before store policy lands. Add the exact budget default, usage/status/range/pin/deletion values, object-safe `RetentionStore` port, registry-backed `PausedBudget` capture state, and `BrowserStatus.retention` composition specified in the parent feature.

## Ordering

First checkpoint. It establishes the values and port consumed by SQLite/store/CDP work.

## Acceptance evidence

- Default budget is exact decimal 10 GB and zero is rejected at constructor and Serde boundaries.
- Usage/status invariants reject overflow, contradictory blocked state, impossible open-overhead claims, and incoherent retained bounds.
- `RetentionStore` is object-safe, domain-only, and covered by the existing core port source guard.
- `PausedBudget` and `BudgetExhausted` guidance round-trip through stable registries without adding an error category.
- Browser status carries retention fields alongside the existing capture cadence and recorded/dropped statistics; it does not duplicate those counters.
