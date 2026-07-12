---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports
kind: story
stage: implementing
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Define structured errors and core infrastructure ports

## Scope

Implement the parent feature's Unit 3: stable structured domain failures and object-safe infrastructure ports for clock, wall time, IDs, browser connection/session, recording persistence, and timeline access.

The browser-facing request/response shape is provisional and may be revised by the next real-browser transport gate. Dependency direction is not provisional: all traits remain in core and adapters depend inward.

## Implementation requirements

- `KrometrailError` carries stable code, safe message, optional domain context, retry advice, and concrete recovery text; arbitrary adapter debug/source text is not serialized.
- `PortFuture<'a, T>` uses only `std::future::Future`, `Pin`, `Box`, and `Send`; core contains no Tokio or `async-trait` type.
- Inject monotonic time, wall time, and raw ID values.
- Keep browser ports capability-shaped and free of CDP/WebSocket/library types.
- Keep recording payload writes and timeline indexing as separate ports.
- Provide deterministic test-only fake adapters and reusable port contract tests.

## Acceptance criteria

- [ ] Every parent Unit 3 signature is implemented or a strictly equivalent safer deviation is recorded.
- [ ] `Arc<dyn Port>` fake adapters compile and exercise success/failure paths without Tokio in core.
- [ ] Structured errors round-trip with stable snake-case codes and safe context.
- [ ] Empty user-facing messages/recovery text fail fast.
- [ ] Metadata/source scans prove no infrastructure-specific type leaks through core.
