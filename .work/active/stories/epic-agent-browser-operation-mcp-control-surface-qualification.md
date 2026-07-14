---
id: epic-agent-browser-operation-mcp-control-surface-qualification
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest from the autopilot caller for final public protocol, schema, binary, and workspace qualification.
- Review weight: `standard` from the autopilot caller; this child advances directly to done and feature review remains separate.
- Files changed: MCP in-memory JSON-RPC qualification, launch-default wire contract, binary interactive JSON-RPC smoke, root test dependency/lock, regenerated current documentation corpus.
- Tests added: real rmcp initialize/initialized/list/start/list-pages/invalid-call/EOF flow over Tokio duplex with fake connector/session, plus real binary initialize/list/EOF framing and stderr separation.
- Simplification: protocol tests derive expected tool counts/names from the core registry rather than a copied operation snapshot; one subprocess test protects both framing and stdout ownership.
- Discrepancies from design: the protocol integration lives as a crate unit test so it can exercise the private server and fake port without widening production visibility; it still crosses rmcp's actual JSON-RPC codec over Tokio duplex.
- Adjacent issues parked: none.

## Completion evidence

- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passed on 2026-07-14.
- `cargo fmt --all -- --check` passed.
- `cargo check --workspace --all-targets --locked` passed.
- `cargo test --workspace --all-targets --locked` passed 418 tests across 38 suites.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed with no issues.
- Focused `cargo +1.85.0 test -p krometrail-mcp --locked` passed 9 MCP tests, including the real in-memory JSON-RPC flow.
- Focused `cargo +1.85.0 test -p krometrail --test rust-runtime-smoke --locked` passed 5 binary tests, including protocol-only initialize/list output and clean pre-initialize EOF.
- `bun run docs:build` regenerated `docs/public/llms-full.txt` and completed the VitePress build.
