---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain
kind: story
stage: implementing
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Implement core capture domain contracts

## Scope

Implement the parent feature's Unit 2 exactly in `krometrail-core`: opaque typed IDs; source, observed, and normalized session time; session/target identities; frame metadata and encoded payload; explicit capture gaps; validated lifecycle transitions; timeline observations; and the capability registry.

Stable domain invariants land now. Chrome timestamp interpretation, transport envelopes, storage locations, MCP schemas, and visual-analysis behavior remain out of scope.

## Implementation requirements

- Use private UUID-backed ID newtypes generated from one implementation registry/macro.
- Use checked integer nanoseconds; expose no implicit arithmetic between unrelated clocks.
- Fail fast on time underflow, invalid ranges/dimensions/scale, empty payloads, invalid transitions, and observation payload-kind mismatch.
- Model every known loss as `CaptureGap`; never imply continuity across missing capture.
- Define capability names/defaults/dependencies/subsystems once; `page-state` and `framework-state` are unavailable.
- Keep core free of Tokio and infrastructure dependencies.

## Acceptance criteria

- [ ] Every parent Unit 2 public signature and invariant is implemented or an implementation note records a strictly equivalent safer signature.
- [ ] IDs cannot be interchanged at compile time and round-trip through display/parse/serde.
- [ ] Tests cover time, range, frame, gap, lifecycle, timeline, and capability success/error paths.
- [ ] Frame metadata preserves source, observed, and session time separately.
- [ ] `cargo test -p krometrail-core` and workspace clippy pass.
