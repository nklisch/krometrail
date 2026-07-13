---
id: epic-agent-browser-operation-browser-page-lifecycle-navigation-observations
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-browser-page-lifecycle
depends_on: [epic-agent-browser-operation-browser-page-lifecycle-selected-page-targets]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Execute navigation and history with anchored live evidence

## Checkpoint

Implement Unit 4 of the parent design through the exact current flat session. Add navigate, reload, back, and forward operations, their bounded operation-specific main-frame commit policy, proactive snapshot invalidation, interaction timing/outcomes, post-operation live observation, and generation-aware cancellation.

Map to `Page.navigate`, `Page.reload`, `Page.getNavigationHistory`, and `Page.navigateToHistoryEntry` exactly as designed. Wait only for bounded loader/URL/history commit evidence; do not add network-idle policy, a permanent lifecycle subscription, generic waits, or automatic replay. Back/forward at a boundary fails before dispatch.

Stop must signal cancellation before queueing shutdown, and transport-pump closure must signal the current connection generation before reconnect input. In-flight transport calls and commit polling must exit without hanging shutdown. Once an interaction is allocated, command rejection, timeout, cancellation, or disconnect remains an anchored failed outcome with an honest observation part.

## Required files

- `crates/krometrail-cdp/src/control/navigation.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/transport/mod.rs`

## Acceptance evidence

- [ ] Navigation/history commands remain exact-session scoped, bounded, and never replay after reconnect.
- [ ] Accepted navigation invalidates the old snapshot generation before live observation; returned URL/history/snapshot/screenshot and interaction times are honest.
- [ ] Same-document, new-document, reload, and history transitions use only their declared commit evidence and make no network-idle claim.
- [ ] History-boundary, malformed reply, `errorText`, timeout, target closure, disconnect, cancellation, and observation-part failures have stable source-safe semantics and no hangs.
- [ ] No generic wait/batch, durable interaction persistence, rich input, MCP, or extra event subscriber enters this checkpoint.

## Ordering

Depends on selected-page target mutations so every operation shares one binding, selection, interaction, and observation path. Qualification follows after the complete workflow exists.
