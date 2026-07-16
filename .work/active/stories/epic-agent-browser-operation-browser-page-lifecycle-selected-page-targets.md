---
id: epic-agent-browser-operation-browser-page-lifecycle-selected-page-targets
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-browser-page-lifecycle
depends_on: [epic-agent-browser-operation-browser-page-lifecycle-lifecycle-profile-status]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Add reducer-owned selected-page and target mutations

## Checkpoint

Implement Unit 3 of the parent design. Add `selected_target_key` to the existing single-writer target supervisor and make its reducer own initial, explicit, fallback, and reconnect selection. Resolve public selection to `TargetId`; never restore or match by URL/title and never add another target registry.

Implement list/create/select/close through the generated browser-operation path. Select must activate Chrome before reducer commit. Create must use the exact `Target.createTarget` key, synchronously feed it through the existing reducer/effect attachment/visibility path, activate/select it, and observe it without waiting on an event queued behind the actor. Close must require Chrome success, synchronously reduce destruction, and return the deterministic attached-page fallback or explicit no-selection state. Later duplicate CDP events must be idempotent.

Inject `IdSource` independently of capture so every state-changing page operation receives an interaction anchor. Reuse the completed live-observation path; closing the last page returns successful change plus an explicitly unavailable observation.

## Required files

- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/targets/supervisor.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/pages.rs`
- `crates/krometrail-cdp/src/session.rs`

## Acceptance evidence

- [ ] Selection always names one live attached exact key or is absent, preserves the same key/ID across reconnect, and falls back deterministically when that renderer disappears.
- [ ] Create reconciles/attaches/selects the exact returned key and duplicate target events cannot duplicate IDs, attachment, capture, or selection events.
- [ ] Select activates before commit; close confirms success before terminal reduction and observes the exact fallback when one exists.
- [ ] List remains read-only/screenshot-free; every attempted state change after target binding returns ordered interaction and honest observation evidence.
- [ ] No second process, profile, lifecycle, reconnect, target, selection, or interaction-storage manager is introduced.

## Ordering

Depends on lifecycle/profile/status because selection is reported through that coherent session contract. Navigation then relies on this one selected/direct resolution path.

## Implementation notes

- Execution capability: highest; exact-key identity, synchronous reconciliation, and duplicate-event behavior are state-machine correctness risks.
- Review weight: standard (caller).
- Files changed: reducer/model selection ownership, page-control helpers, production command dispatch, scripted transport support, and lifecycle qualification.
- Tests: 12 reducer tests plus scripted create/list/select/close qualification prove deterministic initial/explicit/fallback/reconnect selection, activate-before-select, exact-key attachment, browser/session command scope, anchored outcomes, screenshot-free listing, and last-page unavailable observation.
- Simplification: selected page is one optional exact key in the existing reducer; no second target, selection, interaction, or lifecycle store was added.
- Discrepancies from design: `TargetId` allocation remains reducer-owned and deterministic as established by the completed supervision foundation; the injected ID source is used independently for session and interaction IDs.
- Adjacent issues parked: none.
