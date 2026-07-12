---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Bind decisive candidate-contract runs to the scripted wire endpoint

## Reproduction

Strict Linux v2 qualification at commit `1688178f3938876ec4f3aec2a41711b38deace87` failed immediately with `Evidence: scripted candidate contract scenario failed`. The unit contract passes because it constructs `CdpkitTransportFactory::with_scripted_endpoint(server.ws_url)`, while `run_candidate_wire_contract` starts its own server but receives `CdpkitTransportFactory::new()` and never binds the factory to that server.

## Scope

Make the decisive candidate-contract runner construct/use a transport factory bound to the exact scripted server it starts, without coupling the generic scenario contract to cdpkit or bypassing production-real Chrome factory behavior. Preserve one observed wire controller and make this exact decisive path covered by a regression test. Improve failure diagnostics so a scenario failure retains its gate/trace detail.

## Acceptance criteria

- [ ] The exact candidate-contract function used by the real qualification gate talks only to its own scripted server endpoint.
- [ ] A regression test invokes the same decisive function/factory path and proves all wire-authentic results.
- [ ] Scenario failure diagnostics retain useful underlying evidence.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change lands.
