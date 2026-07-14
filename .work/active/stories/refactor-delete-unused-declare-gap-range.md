---
id: refactor-delete-unused-declare-gap-range
kind: story
stage: done
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

- [x] `StreamRuntime::declare_gap_range` and its doc comment are removed from `crates/krometrail-cdp/src/capture/pipeline.rs`.
- [x] `grep -rn "declare_gap_range" crates/` returns zero matches.
- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo test --workspace --all-targets --locked` passes (capture tests cover the live `declare_gap` path; nothing exercised `declare_gap_range`).
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes

- Execution capability: raised — inherited from the active autopilot run; the edit is tiny, but it touches the capture accounting path.
- Review weight: standard (source: autopilot default).
- Files changed: `crates/krometrail-cdp/src/capture/pipeline.rs` (deletion only); this story file.
- Tests added/removed: none — the deleted method had no callers, so no coverage is lost.
- Simplification: removed the unreachable ordered-range gap construction, counter update, ledger insertion, and observer notification; `declare_gap` remains the sole live declaration path.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: zero remaining workspace matches plus locked workspace format, check, test, and Clippy gates passed in an isolated clean worktree at `a8976ca`, whose capture pipeline is byte-identical to the current base. Concurrent verified-interaction work in the shared tree was excluded rather than modified.

## Risk and rollback

**Risk**: Low. Private method with zero callers; deletion cannot change runtime behavior.

**Rollback**: Revert the deletion commit.

## Discovery notes

- Scope: autopilot `--all` refactor cadence, group 1 (CDP capture foundation) — `crates/krometrail-cdp/src/{capture,targets,transport,launcher}` and corresponding core contracts/tests.
- Dispatch: direct read of `pipeline.rs` + workspace grep; no subagents, no peeragent (cadence is conservative, local-only by directive).
- Value: medium — clean dead-weight removal of an unreachable duplicate of the live gap-declaration path; small but unambiguous.
- Adjacent code considered and not flagged: the recurring `matches!(target.target.lifecycle, TargetLifecycle::Closed | TargetLifecycle::Failed)` guard (14 sites workspace-wide) was considered for a `TargetLifecycle::is_terminal()` helper but recorded as not-worth-it — it is a one-line idiomatic check and renaming it does not materially reduce complexity.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Bounded inline standalone-story review. The commit deletes only the private zero-caller
method and its duplicate body; the live `declare_gap` path is unchanged. Workspace search, isolated
locked gates, and direct diff inspection confirm behavior preservation. No independent or
cross-model reviewer ran, as required for standalone stories.
