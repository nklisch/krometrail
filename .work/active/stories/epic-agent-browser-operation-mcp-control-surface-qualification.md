---
id: epic-agent-browser-operation-mcp-control-surface-qualification
kind: story
stage: implementing
tags: [browser, agent-ux, testing]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-stdio-wiring]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Protocol and Runtime Qualification

## Checkpoint

Close the public MCP boundary with registry/schema drift tests, typed wire validation, response/error/image coverage, an in-memory JSON-RPC protocol smoke, and real binary stdio/help/runtime contracts.

## Likely files

- `crates/krometrail-mcp/src/*` focused unit tests
- `crates/krometrail-mcp/tests/protocol.rs` or equivalent duplex-transport integration test
- `tests/rust-runtime-smoke.rs`
- only bounded integration fixes discovered by this evidence

## Acceptance evidence

- Registration tests compare enabled tools and batch branches to `BROWSER_OPERATION_REGISTRY` and capability metadata rather than a copied 24-name snapshot. Disabled `Control` omission and conservative annotations are asserted.
- Typed MCP argument round trips cover a simple request, validated interaction, integer-millisecond wait, and nested batch; malformed input receives a visible stable error and reaches the fake session zero times.
- Response tests cover success, degradation, stable error, interaction anchor, PNG/JPEG separation, wait timeout, and partial batch failure without testing every trivial mapping.
- A Tokio duplex/in-memory protocol test performs MCP initialize, `tools/list`, one valid `tools/call`, one invalid call, and shutdown against a fake connector/session; responses are real JSON-RPC frames and no task leaks.
- A subprocess test runs `krometrail mcp`, asserts every stdout line is protocol JSON, verifies stderr separation, and closes stdin to prove clean EOF shutdown. Binary help/runtime contracts remain truthful.
- `cargo +1.85.0 check --workspace --all-targets --locked`, `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Existing browser-control qualification stays green.
- The story records exact commands, test counts, and any honest platform limitation before advancing directly to `done`.

## Out of scope

No paid-agent evaluation, temporal artifact evaluation, real-browser requalification of already-proven operation semantics unless MCP evidence exposes an integration defect, network transport, storage, page/framework state, or test weakening.
