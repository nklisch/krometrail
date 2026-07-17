---
id: epic-agent-browser-reliability-viewport-emulation-public-contract
kind: story
stage: done
tags: [browser, agent-ux, visual]
parent: epic-agent-browser-reliability-viewport-emulation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Add the explicit viewport operation and target command

## Checkpoint

Add the validated, selected-target-defaulted `set_viewport` override/clear operation, normal
page-operation/timeline result, transactional CDP commands, and independently observed effective
metrics. This checkpoint owns the public/browser-control portion of GitHub issue #10.

## Acceptance evidence

- [x] Generated MCP schema accepts bounded explicit metrics or clear and preserves all existing
      operation contracts.
- [x] Override and clear affect only the selected target and report declared mobile state plus
      observed CSS size, device scale, and touch state.
- [x] Partial command or effective-observation failure restores the prior state when possible and
      never commits a false success.
- [x] A successful result carries ordinary live evidence and a persisted timeline anchor without
      creating a navigation identity.

## Ordering and blocker

Independent first checkpoint. Its validated types and command semantics are prerequisites for
supervisor restoration and temporal geometry-transition evidence.

## Implementation evidence

- Added constructor-backed `ViewportMetrics`, explicit tagged override/clear requests, effective
  metrics, the additive registry-derived 25th operation, and the normal page-change anchor.
- Added target-session CDP metrics/touch application, independent runtime/layout observation,
  mismatch rejection, best-effort prior-state rollback, ordinary live observation, batch support,
  temporal evidence projection, and MCP response projection.
- Verified with `cargo check --workspace --all-targets --offline`, three core viewport contract
  tests, and scripted CDP command/effective-state tests.
