---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness
kind: story
stage: done
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

- [x] The default crate remains a truthful empty production adapter boundary and has no selected CDP dependency.
- [x] Both `FakeTransport` and a candidate adapter can consume the same `run_transport_scenarios` suite; no second hand-maintained scenario list exists.
- [x] Fake routing, drift, ordering, disconnect, reconnect, and session rebuild tests are deterministic and contain no `sleep` calls.
- [x] The schema is generated, versioned as `1`, and round-trip/negative tests prove strict validation and secret/path redaction.
- [x] `cargo fmt --all --check`, default workspace check/test/clippy, and `cargo test -p krometrail-cdp --features cdp-spike --test transport_contract` pass.

## Implementation notes
- Execution capability: highest-tier direct implementation; the caller prohibited questions and subagents, and the bounded crate/filesystem scope had clear ownership.
- Review weight: standard, inherited from the active autopilot policy; implementation stops at `stage: review` for the requested handoff.
- Dispatch rationale: direct-read only. The parent design, research reference, existing workspace manifests, and adapter boundary answered the integration questions without exploratory delegation.
- Files changed: root `Cargo.toml` and `Cargo.lock`; `crates/krometrail-cdp/Cargo.toml`, `src/lib.rs`, all six spike modules, contract tests, and three protocol fixtures; generated `docs/evidence/cdp-transport/v1/schema.json`.
- Tests added: deterministic fake/candidate-neutral scenario coverage, local in-memory WebSocket framing, disconnect/rebuild behavior, fixture loading, strict evidence round-trip/negative validation, and generated-schema parity.
- Discrepancies from design: the scripted peer uses a real Tokio WebSocket framing pair over an in-memory duplex stream rather than binding a machine port, preserving deterministic no-port behavior; no candidate adapter was added because this story owns only the harness.
- Adjacent issues parked: none.
- Verification: default workspace fmt/check/test/clippy; `cdp-spike` fmt/check/test/clippy; `cdp-spike-cdpkit` check/test/clippy; dependency-tree isolation confirms cdpkit is absent from the default graph and present only with its feature.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran the default 41-test gate, seven deterministic spike-contract tests, candidate-feature clippy, and dependency isolation. The disposable boundary and generated evidence contract are green. Verdict: Approve - story verified by implement; fast-lane advance.
