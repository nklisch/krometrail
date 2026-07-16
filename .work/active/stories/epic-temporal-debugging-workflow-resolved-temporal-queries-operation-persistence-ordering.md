---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-operation-persistence-ordering
kind: story
stage: done
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Fence browser success on durable evidence

## Checkpoint

Wire one `InteractionEvidenceSink` into the production CDP session and centralize evidence projection in the shared non-batch operation path. Persist every returned state-changing anchor before standalone publication or the next batch step, without moving persistence into MCP or action-family handlers.

## Required contract

- `ProductionBrowserConnector::with_interaction_evidence` injects the sink; state-changing work without one fails before dispatch while read-only work remains available.
- Page-operation results persist their existing anchor and no fabricated record; browser-action results persist `InteractionResult::anchor()` plus the exact existing record.
- Only successful explicit navigate/reload/back/forward results mint a `NavigationId` and project a completion-time navigation point.
- Batch children recurse through the same persistence fence; the outer batch is not projected again, and existing parent-batch IDs remain record references rather than new interaction rows.
- Persistence failure after a browser effect returns `PersistenceFailed` with interaction context, retry `Never`, and inspect-before-repeat recovery. It never becomes a successful degraded result or an automatic action retry.

## Acceptance evidence

- [ ] A delayed sink proves standalone result publication waits for commit; read-only requests never call it.
- [ ] A two-step batch proves step 2 cannot dispatch before step 1 evidence commits.
- [ ] A failed sink produces a failed step and default stop-on-failure behavior without duplicate rows or replayed input.
- [ ] Exhaustive result handling projects every page/action variant exactly once and navigation only for successful explicit navigation controls.

## Ordering

Depends on the durable store implementation so the integration is designed against the real transaction boundary, not a temporary production cache.

## Implementation notes

- Execution capability: highest; retained by caller because browser effects, batch cancellation/deadlines, and durable publication form one failure-sensitive ordering boundary.
- Review weight: standard, from the autopilot caller; child checkpoint review is not applicable.
- Files changed: `crates/krometrail-cdp/src/session/{evidence.rs,mod.rs,operations.rs}` and focused CDP test/support files, including `tests/temporal_evidence.rs` and batch ordering coverage.
- Tests added/updated: missing-sink pre-dispatch rejection with read-only availability; delayed standalone sink publication fence; post-effect sink failure remapping; two-step batch gate proving step 2 input cannot dispatch before step 1 commit; default batch stop on persistence failure; successful explicit navigation ID qualification; all existing scripted and opt-in state-changing fixtures now inject deliberate test evidence sinks.
- Semantics delivered: one shared non-batch fence; exact page anchor/action record projection; no outer-batch row; successful explicit navigate/reload/back/forward navigation IDs only; failure context includes session/target/interaction with retry `Never` and inspect-before-repeat recovery; no sink means no state-changing CDP dispatch.
- Simplification: evidence projection is centralized once after non-batch dispatch, rather than duplicated across page/action families or MCP; exhaustive `BrowserOperationResult` matching keeps new variants compile-visible.
- Discrepancies from design: persistence is awaited directly rather than detached or outboxed. Existing batch cancellation/deadline can stop publication while never advancing another step; this preserves the designed synchronous fence and avoids a second eventual-consistency mechanism.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; `cargo test -p krometrail-cdp --all-targets --locked` (221 passed across 19 suites); `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings`. No opt-in live-Chrome run was claimed.
