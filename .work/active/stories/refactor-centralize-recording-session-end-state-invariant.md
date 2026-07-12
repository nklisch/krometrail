---
id: refactor-centralize-recording-session-end-state-invariant
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

# Centralize the recording-session end-state invariant

## Brief

`crates/krometrail-core/src/recording/session.rs:230-242` validates the relationship between `SessionLifecycle`, `started_at`, and `ended_at` when constructing or deserializing a session. `RecordingSession::transition` repeats the same three invalid cases at `session.rs:296-307` before mutating the aggregate.

Extract one private invariant validator for the candidate lifecycle/end-time pair. Call it from aggregate validation and from `transition` before assigning either field, then perform the mutation only after validation succeeds. Preserve the current lifecycle-transition check ordering and exact error messages.

**Source lens**: missing abstraction / domain invariant duplication

**Rationale**: makes one function authoritative for the rule that only ended sessions have an end time and that the end cannot precede the start, preventing construction and mutation paths from drifting.

**Black-box classification**: pure refactor. Public types, method signatures, accepted/rejected transitions, error codes/messages, serialization, and atomic-on-error mutation behavior remain unchanged.

## Acceptance criteria

- [x] One private helper owns all lifecycle/start/end-time consistency checks in `recording/session.rs`.
- [x] `RecordingSession::validate` and `RecordingSession::transition` both use that helper; the transition validates before mutating `lifecycle` or `ended_at`.
- [x] Existing error messages and validation ordering remain unchanged.
- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo test --workspace --all-targets --locked` passes, including malformed deserialization and lifecycle transition coverage.
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes
- Files changed: `crates/krometrail-core/src/recording/session.rs`; this story file.
- Tests added: none; existing malformed-session and lifecycle-transition coverage passed unchanged.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- `validate_lifecycle_end_state` now owns the shared candidate-state checks. Transition ordering remains lifecycle-transition validation first, invariant validation second, and both aggregate fields are assigned only after the invariant succeeds.

## Risk and rollback

**Risk**: Low. The duplicated match arms are local and have identical invalid cases, but mutation ordering must remain atomic on failure.

**Rollback**: Revert the implementation commit to restore the two inline invariant checks.

## Discovery notes

- Scope: second mandatory five-story autopilot cadence; distribution workflows/scripts/manifests/static contract tests, current contributor/docs navigation surfaces, and remediation-touched core invariant/enum modules.
- Dispatch: direct-read only as required; no questions or subagents. `.pi/`, escalated review metadata, and the existing `refactor-derive-cli-error-code-names` finding were excluded.
- Value: high — this removes duplicate aggregate invariants from two state-entry paths rather than merely shortening syntax.
