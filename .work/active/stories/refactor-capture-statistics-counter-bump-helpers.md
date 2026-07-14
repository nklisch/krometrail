---
id: refactor-capture-statistics-counter-bump-helpers
kind: story
stage: implementing
tags: [refactor, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-13
updated: 2026-07-13
---

# Add per-counter bump helpers to `CaptureStatistics`

## Brief

`crates/krometrail-core/src/recording/session.rs` defines `CaptureStatistics`
as a six-field validated aggregate (`received_frames`, `acknowledged_frames`,
`accepted_frames`, `dropped_frames`, `persisted_frames`, `gap_count`) with a
single fallible constructor `CaptureStatistics::new(...)` and a same-shape
`update(...)` mutator that takes all six fields.

`crates/krometrail-cdp/src/capture/pipeline.rs` rebuilds the entire struct
from accessors six times in order to bump exactly one counter by
`saturating_add(1)`. Each site is the same shape:

```rust
state.statistics = CaptureStatistics::new(
    state.statistics.received_frames(),
    state.statistics.acknowledged_frames().saturating_add(1), // <- one field bumped
    state.statistics.accepted_frames(),
    state.statistics.dropped_frames(),
    state.statistics.persisted_frames(),
    state.statistics.gap_count(),
)
.expect("capture counters cannot overflow in a bounded process");
```

The six live sites (after the separate dead-`declare_gap_range` deletion lands)
are in `StreamRuntime::record_received`, `record_ack`, `handoff` (Ok arm),
`dropped`, `persisted`, and `declare_gap` — one per counter. Each must be kept
in sync with the constructor's field order; adding a seventh field would force
six coordinated edits, and the existing `update(...)` mutator does not help
because it still requires passing all six fields.

Add one `Result<Self>`-returning helper per counter on `CaptureStatistics`
(e.g. `record_received`, `record_acknowledged`, `record_accepted`,
`record_dropped`, `record_persisted`, `record_gap`) that performs the
`saturating_add(1)` and re-validates via the existing `validate()` path, then
collapse each pipeline.rs site to a one-line
`state.statistics = state.statistics.record_X().expect("<existing message>")`.

**Source lens**: missing abstraction / single-source-of-truth violation

**Rationale**: makes the constructor the single source of the field list for
single-counter mutations; the six rebuild sites become six one-liners that
cannot drift from the field order.

**Black-box classification**: pure refactor. `CaptureStatistics` gains
infallbackible-result helper methods (non-breaking addition); the existing
`new`, `update`, accessors, `validate`, `Deserialize`, and serialization are
unchanged. Each call site preserves its exact `saturating_add(1)` semantics,
its exact re-validation path, and its exact `.expect(...)` message, so no
caller-observable behavior changes.

## Acceptance criteria

- [ ] `CaptureStatistics` exposes one `Result<Self>` helper per counter that performs `saturating_add(1)` and re-validates.
- [ ] All six live rebuild sites in `crates/krometrail-cdp/src/capture/pipeline.rs` (`record_received`, `record_ack`, `handoff` Ok arm, `dropped`, `persisted`, `declare_gap`) use the new helpers.
- [ ] Each call site preserves its existing `.expect(...)` invariant message.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes, including capture counter and statistics-validation coverage.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes

- Files changed: `crates/krometrail-core/src/recording/session.rs` (add helpers near `update`); `crates/krometrail-cdp/src/capture/pipeline.rs` (six call-site collapses); this story file.
- Tests added: none required — existing capture tests exercise every counter through the ingestion pipeline and the validation invariants are covered by `recording/session.rs` tests. Add a unit test only if a counter's bump path is not otherwise exercised.
- The helpers should consume `self` (the struct is `Copy`) and return `Result<Self>` so callers can keep the existing `.expect(...)` panic-message contract.
- Land after `refactor-delete-unused-declare-gap-range` so the dead `declare_gap_range` rebuild site is already gone and only the six live sites are updated.

## Risk and rollback

**Risk**: Low. The struct is `Copy`, the validation path is reused unchanged, and each call site's panic message is preserved at the call site. The only structural risk is mis-mapping a helper to the wrong counter, which the existing capture tests (which assert per-counter accounting through ack/drop/persist/gap paths) will catch.

**Rollback**: Revert the implementation commit to restore the six inline `CaptureStatistics::new(...)` rebuilds.

## Discovery notes

- Scope: autopilot `--all` refactor cadence, group 1 (CDP capture foundation) — `crates/krometrail-cdp/src/{capture,targets,transport,launcher}` and corresponding core contracts/tests (`CaptureStatistics` lives in `crates/krometrail-core/src/recording/session.rs`).
- Dispatch: direct read of `pipeline.rs` + `recording/session.rs`; no subagents, no peeragent (cadence is conservative, local-only by directive).
- Value: medium — removes a real single-source-of-truth violation across six live call sites that must track a six-field constructor by hand; not merely cosmetic.
- Existing `refactor-centralize-recording-session-end-state-invariant` (done) covered `SessionLifecycle`/end-time validation in the same file and does not overlap this counter-bump finding.
