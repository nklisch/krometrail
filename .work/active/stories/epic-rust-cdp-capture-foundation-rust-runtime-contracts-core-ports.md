---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports
kind: story
stage: done
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain]
release_binding: 1.0.0
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

## Implementation notes

- Files changed: `crates/krometrail-core/src/error.rs`, `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-core/src/ports/clock.rs`, `crates/krometrail-core/src/ports/ids.rs`, `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-core/src/ports/recording.rs`, `crates/krometrail-core/src/ports/timeline.rs`, `crates/krometrail-core/src/capabilities/mod.rs`, and `crates/krometrail-core/src/lib.rs`.
- Tests added: structured error serde/context/recovery tests plus deterministic object-safe fake clock, wall-clock, ID, browser, recording, and timeline port contract tests (including success and structured failure paths), a std-only future executor, and source/manifest leak assertions.
- Discrepancies from design: `NonEmptyText` stores a boxed string rather than an exposed `String` to keep the richer `KrometrailError` below clippy's large-error threshold; serde shape is unchanged. Its manual deserializer additionally rejects empty serialized text, which is required by the fail-fast boundary contract. New error fields have serde defaults so Unit 2's `{code,message}` error payloads remain readable.
- Dispatch: direct local reads only, per caller instruction; no subagents used.
- Adjacent issues parked: none.
- Verification: dependency `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain` confirmed `stage: done` via `.work/bin/work-view --stage done --paths`. `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets -- -D warnings` all pass. A repository scan finds no Tokio, async-trait, WebSocket, SQLite, or CDP marker in `krometrail-core`; Cargo metadata reports only serde, thiserror, uuid, and dev-only serde_json dependencies.

## Acceptance criteria

- [x] Every parent Unit 3 signature is implemented or a strictly equivalent safer deviation is recorded.
- [x] `Arc<dyn Port>` fake adapters compile and exercise success/failure paths without Tokio in core.
- [x] Structured errors round-trip with stable snake-case codes and safe context.
- [x] Empty user-facing messages/recovery text fail fast.
- [x] Metadata/source scans prove no infrastructure-specific type leaks through core.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The implementation record reports the complete workspace gate green; the orchestrator independently reran formatting, 26 core tests, and workspace clippy successfully. Verdict: Approve - story verified by implement; fast-lane advance.
