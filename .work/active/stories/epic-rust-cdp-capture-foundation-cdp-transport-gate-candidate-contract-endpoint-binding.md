---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding
kind: story
stage: done
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

## Root cause

`run_candidate_wire_contract` owned the scripted server but accepted an already-created factory, so the real gate passed `CdpkitTransportFactory::new()` and connected to the literal `scripted-peer` URL instead of the server it had just started. The unit test avoided the bug by binding its factory before invoking the generic scenarios.

## Fix approach

The decisive helper now accepts a candidate-neutral factory constructor closure, starts its scripted server first, and invokes the closure with that server's endpoint. The cdpkit-specific caller constructs `CdpkitTransportFactory::with_scripted_endpoint` for this path only; the original factory remains unchanged for real Chrome. Scenario failure errors include the complete gate registry and execution trace.

## Regression test

`crates/krometrail-cdp/tests/cdpkit_transport_contract.rs` invokes `run_candidate_wire_contract` itself and asserts every `CandidateContractResults` wire-authentic result, including drift survival, session routing, ordering, disconnect cleanup, socket closure, and explicit rebuild evidence.

## Acceptance criteria

- [x] The exact candidate-contract function used by the real qualification gate talks only to its own scripted server endpoint.
- [x] A regression test invokes the same decisive function/factory path and proves all wire-authentic results.
- [x] Scenario failure diagnostics retain useful underlying evidence.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change lands.

## Implementation notes

- Execution capability: inline host implementation; this was a focused spike-harness boundary and regression-test change with no safe ownership split or dispatch need.
- Review weight: `standard`, from the project default; no caller override was supplied.
- Changed files: `crates/krometrail-cdp/src/spike/chrome_harness.rs`, `crates/krometrail-cdp/src/spike/mod.rs`, `crates/krometrail-cdp/src/spike/scenarios.rs`, and `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs`.
- Verification: `cargo fmt --all -- --check`; default workspace tests; `cdp-spike` tests; `cdp-spike-cdpkit` tests; and denied-warning clippy for default, spike, and cdpkit configurations all passed.
- Deliberately not run: fresh 60-second evidence qualification. No production/core changes, push, or dispatch were made. No adjacent issues were parked.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane verified-bug review confirmed the exact decisive helper binds its own scripted endpoint through a candidate-neutral constructor, retains the separate real-Chrome factory, exposes underlying scenario evidence on failure, and is covered by the same function used in qualification. All 24 candidate-feature tests and denied-warning clippy pass. Verdict: Approve - story verified by implement; fast-lane advance.
