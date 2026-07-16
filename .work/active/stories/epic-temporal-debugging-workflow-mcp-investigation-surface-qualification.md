---
id: epic-temporal-debugging-workflow-mcp-investigation-surface-qualification
kind: story
stage: done
tags: [agent-ux, visual, browser, storage, testing]
parent: epic-temporal-debugging-workflow-mcp-investigation-surface
depends_on:
  - epic-temporal-debugging-workflow-mcp-investigation-surface-resource-server-and-root-composition
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal MCP Investigation Workflow Qualification

## Checkpoint

Qualify the complete local agent path over real rmcp JSON-RPC framing: interaction anchor, temporal bundle, cached/partial evidence, event/source/region drill-down, durable resource reads, pinning, cancellation, eviction, and deletion. This is deterministic product qualification, not the paid multimodal thesis benchmark.

## Likely files

- `crates/krometrail-mcp/tests/protocol.rs` or the existing crate-local protocol tests
- `crates/krometrail-mcp/src/{registry.rs,response.rs,resources.rs,server.rs}` tests
- `src/app.rs` tests
- existing temporal bundle/progressive/store fixtures
- `tests/rust-runtime-smoke.rs` only for truthful MCP stdout/lifecycle assertions

## Design

Use one schema-v5 recording fixture with a changing and unchanged interval, one interaction anchor, cacheable artifacts, a declared capture gap, browser events, a fixed region, selected source frames, and exact/overlapping pin state. Drive the server through Tokio duplex streams and use barriers for artifact generation, resource reads, eviction, cancellation, and session deletion. Avoid sleeps, stopwatch assertions, large image snapshots, and paid model calls.

## Acceptance evidence

- [ ] Real initialize/tools/list/resources/templates/list/tools/call/resources/read traffic is valid rmcp 0.11 traffic at 2025-06-18 and stdout contains only protocol frames.
- [ ] The bundle path proves one natural resolution, exact resolved-range propagation, generated then cache-hit outcomes, compact event proximity, explicit gaps/degradations, deterministic primary-image/resource-link projection, and no diagnosis/causality language.
- [ ] Region, source-frame, artifact-variant, chronological-event, and pin tools consume the exact resolved scope/range and preserve their domain ordering/retention semantics.
- [ ] Resource reads return original retained bytes with exact MIME/hash/length; eviction, invalidation, wrong scope, deletion, malformed URI, cancellation, and corruption return no stale or partial blob.
- [ ] Capability filtering is independent: temporal-vision/browser-events toggles affect only their routes/templates, while ordinary control remains intact.
- [ ] Root/service tests prove one store/artifact/cache/service authority and no mutation gate spans resource file I/O or visual generation.
- [ ] Rust 1.85 locked format/check/test/Clippy gates pass; tests protect protocol, resource lifetime, cancellation, and response contracts rather than trivial wrappers or every enum branch.

## Ordering constraints

This is the final checkpoint and depends on the complete server/resource/root composition. It advances the parent feature only after the integrated local workflow is green.

## Implementation notes

- Execution capability: inline feature-owner qualification pass; the final checkpoint uses the existing real rmcp/Tokio duplex harness and lower-layer schema-v5 authorities rather than introducing another runtime or fixture format.
- Review weight: standard, default; this child checkpoint advances directly to done and the parent feature is left at review without a parent review pass.
- Files changed: `crates/krometrail-mcp/src/{server.rs,resources.rs}` test surfaces.
- Tests added: exact retained blob/error/cancellation resource-authority cases, full MCP initialize/resource/template/EOF wire coverage, control-only template omission, and exact typed source-frame/fetch/artifact/region/pin/event route propagation.
- Simplification: reused the existing protocol helpers, strict resource authority, request cancellation bridge, and lower-layer schema-v5/store qualification tests; no second fixture, cache, decoder, or protocol runtime was introduced.
- Discrepancies from design: none; the existing artifact/progressive/store schema-v5 qualifications remain the lower-layer authorities for generation/cache, retention, corruption, and mutation-gate behavior.
- Adjacent issues parked: none.

## Verification evidence

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (636 passed, 1 ignored)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Out of scope

No implementation of browser control, temporal algorithms, remote MCP, task semantics, paid multimodal evaluation, automatic diagnosis, or cross-session comparison.
