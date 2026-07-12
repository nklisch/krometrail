---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness
kind: story
stage: implementing
tags: [browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Build the disposable transport contract and deterministic harness

## Scope

Add the non-default `krometrail-cdp` spike surface, one candidate-neutral `SpikeTransport` contract, the versioned evidence/error model, a deterministic in-memory fake, a scripted local WebSocket peer, and the shared routing/drift/disconnect scenario suite. This is test and qualification scaffolding only: keep the existing truthful production adapter boundary empty and do not add a production transport, core-port revision, sixth crate, or `unimplemented!` placeholder.

## Exact files

- `Cargo.toml`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/spike/mod.rs`
- `crates/krometrail-cdp/src/spike/contract.rs`
- `crates/krometrail-cdp/src/spike/error.rs`
- `crates/krometrail-cdp/src/spike/evidence.rs`
- `crates/krometrail-cdp/src/spike/fake.rs`
- `crates/krometrail-cdp/src/spike/scripted_peer.rs`
- `crates/krometrail-cdp/src/spike/scenarios.rs`
- `crates/krometrail-cdp/tests/transport_contract.rs`
- `crates/krometrail-cdp/tests/fixtures/protocol/additive-field.json`
- `crates/krometrail-cdp/tests/fixtures/protocol/unknown-enum.json`
- `crates/krometrail-cdp/tests/fixtures/protocol/unknown-event.json`
- `docs/evidence/cdp-transport/v1/schema.json` (generated from Rust evidence types)

## Requirements

- Gate all spike modules behind non-default `cdp-spike`; gate `cdpkit` code separately behind `cdp-spike-cdpkit`. Add exact `cdpkit = "=0.4.0"` at the workspace dependency source of truth, optional in `krometrail-cdp`; `cargo check --workspace --all-targets` without spike features must not compile or select it.
- Use one object-safe spike-only transport/scenario API for the deterministic fake and every candidate adapter. Preserve the honest raw boundary as `NamedEventParams { method, scope, params }`; do not model or claim a wildcard/full-envelope stream that `cdpkit` cannot provide.
- Model all gate identifiers once in `TransportGateId::ALL`; evidence validation and decision logic derive from that registry.
- Deterministically exercise two flat sessions, same-named event isolation, event-before-response, detach-during-command, browser/session raw calls, unknown named events, additive fields, unknown enum values, pending-call disconnect, subscription closure, and explicit reconnect/rebuild. Coordinate with scripted messages, barriers, and oneshot channels; no sleeps or timing races are permitted in fake scenarios. Timeouts may only fail a hung test, not order it.
- Generate JSON Schema from the Rust evidence types. Reject unknown schema versions, duplicate/missing gate IDs, non-finite measurements, leaked absolute paths/endpoints, and pass results that lack required measurements.

## Acceptance criteria

- [ ] The default crate remains a truthful empty production adapter boundary and has no selected CDP dependency.
- [ ] Both `FakeTransport` and a candidate adapter can consume the same `run_transport_scenarios` suite; no second hand-maintained scenario list exists.
- [ ] Fake routing, drift, ordering, disconnect, reconnect, and session rebuild tests are deterministic and contain no `sleep` calls.
- [ ] The schema is generated, versioned as `1`, and round-trip/negative tests prove strict validation and secret/path redaction.
- [ ] `cargo fmt --all --check`, default workspace check/test/clippy, and `cargo test -p krometrail-cdp --features cdp-spike --test transport_contract` pass.
