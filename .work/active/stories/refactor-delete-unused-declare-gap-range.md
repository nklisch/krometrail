---
id: refactor-delete-unused-declare-gap-range
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

# Delete the unused `StreamRuntime::declare_gap_range` helper

## Brief

`crates/krometrail-cdp/src/capture/pipeline.rs:491-520` defines
`StreamRuntime::declare_gap_range`, a private method that constructs a
`CaptureGap` over an ordered `(start, end)` session range, updates capture
statistics, pushes the gap into the ledger, and notifies the observer. A
workspace-wide search (`grep -rn "declare_gap_range" crates/`) returns zero
callers — only the definition site matches. The sibling single-point
`declare_gap` (pipeline.rs:456) is the method every call site uses (eight
callers in pipeline.rs).

The `declare_gap_range` body (~30 lines including the doc comment) is dead
weight: it cannot be reached from production code or from the capture test
module (`crates/krometrail-cdp/src/capture/tests.rs` has zero references).

**Source lens**: dead weight (zero callers workspace-wide)

**Rationale**: removes ~30 lines of unreached gap-construction,
statistics-update, and observer-notification code that duplicates the live
`declare_gap` path and would otherwise have to be kept in sync with
`CaptureStatistics`/`GapLedger`/`CaptureObserver` changes despite never
running.

**Black-box classification**: pure refactor (deletion). No public surface
references the method (it is private), no caller behavior changes, and no
serialized form is affected. The live `declare_gap` path is untouched.

## Acceptance criteria

- [ ] `StreamRuntime::declare_gap_range` and its doc comment are removed from `crates/krometrail-cdp/src/capture/pipeline.rs`.
- [ ] `grep -rn "declare_gap_range" crates/` returns zero matches.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes (capture tests cover the live `declare_gap` path; nothing exercised `declare_gap_range`).
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes

- Files changed: `crates/krometrail-cdp/src/capture/pipeline.rs` (deletion only); this story file.
- Tests added: none — the deleted method had no callers, so no coverage is lost.
- The sibling `declare_gap` (pipeline.rs:456) remains the sole gap-declaration path; do not modify it.
- If a future caller needs an ordered-range gap, it should be added back with a call site in the same change, not kept speculatively.

## Risk and rollback

**Risk**: Low. Private method with zero callers; deletion cannot change runtime behavior.

**Rollback**: Revert the deletion commit.

## Discovery notes

- Scope: autopilot `--all` refactor cadence, group 1 (CDP capture foundation) — `crates/krometrail-cdp/src/{capture,targets,transport,launcher}` and corresponding core contracts/tests.
- Dispatch: direct read of `pipeline.rs` + workspace grep; no subagents, no peeragent (cadence is conservative, local-only by directive).
- Value: medium — clean dead-weight removal of an unreachable duplicate of the live gap-declaration path; small but unambiguous.
- Adjacent code considered and not flagged: the recurring `matches!(target.target.lifecycle, TargetLifecycle::Closed | TargetLifecycle::Failed)` guard (14 sites workspace-wide) was considered for a `TargetLifecycle::is_terminal()` helper but recorded as not-worth-it — it is a one-line idiomatic check and renaming it does not materially reduce complexity.
