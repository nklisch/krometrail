---
id: refactor-generate-observation-kind-payload-contract
kind: story
stage: review
tags: [refactor]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-12
updated: 2026-07-12
---

# Generate the observation kind/payload compatibility contract

## Brief

`crates/krometrail-core/src/timeline/observation.rs:12-23` declares every `ObservationKind`, `observation.rs:27-34` separately declares payload-reference variants, and `observation.rs:105-125` enumerates the legal pairings a third time. Adding or renaming a timeline observation category therefore requires coordinated edits across three growing registries.

Replace those parallel declarations with one local declarative macro invocation that generates the existing public `ObservationKind` and `ObservationPayloadRef` enums plus their compatibility predicate. The declaration must distinguish typed one-to-one payloads from the existing kinds that use `External(String)`. Preserve enum variants, derives, serde tagging/renaming, public constructor/accessor signatures, exact serialized forms, and validation messages. Do not infer `kind` from `payload` or alter the wire contract.

**Source lens**: missing abstraction / single source of truth

**Rationale**: makes the growing observation taxonomy and its legal payload association one registry while retaining the deliberately redundant public wire fields used for fail-fast validation.

**Black-box classification**: pure refactor. The same kind/payload pairs remain valid or invalid, and all public Rust and Serde contracts remain byte-for-byte/source-compatible.

## Acceptance criteria

- [x] One declaration in `timeline/observation.rs` generates both existing enums and the complete legal kind/payload compatibility match.
- [x] All current enum variant names, trait derives, serde names/tag/content fields, constructors, accessors, and validation errors remain unchanged.
- [x] External payload non-empty validation and session-time ordering validation remain independent and unchanged.
- [x] Tests exhaustively cover every generated legal association and representative illegal associations without hand-copying the registry again.
- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo test --workspace --all-targets --locked` passes.
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes
- Files changed: `crates/krometrail-core/src/timeline/observation.rs`; this story file.
- Tests added: a macro-generated contract test enumerates every generated payload and asserts each generated kind has exactly one compatible payload, covering all legal and nonmatching associations without a second hand-maintained registry.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- The local declaration preserves the original enum order, derives, Serde attributes, payload variants, validation messages, and independent external-payload/time checks while generating the compatibility predicate.

## Risk and rollback

**Risk**: Medium. Macro output sits on a public serialized boundary, so accidental derive, rename, or enum-shape drift would be observable despite the structural intent.

**Rollback**: Revert the implementation commit to restore the explicit enums and compatibility match.

## Discovery notes

- Scope: second mandatory five-story autopilot cadence; distribution workflows/scripts/manifests/static contract tests, current contributor/docs navigation surfaces, and remediation-touched core invariant/enum modules.
- Dispatch: direct-read only as required; no questions or subagents. `.pi/`, escalated review metadata, and the existing `refactor-derive-cli-error-code-names` finding were excluded.
- Value: high — a ten-kind public taxonomy currently has three coordinated sources of truth; generation removes that drift risk without changing its boundary.
