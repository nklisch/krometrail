---
id: epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause
kind: story
stage: done
tags: [browser, storage, diagnostics]
parent: epic-agent-surface-simplification-persistence-recovery
depends_on: [epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Preserve the first classified persistence cause in capture health

Replace capture's stage-only terminal state with one validated `CaptureFailure` containing the stage and first sanitized cause. Pass frame/gap sink errors through instead of discarding them, preserve first-failure semantics, and emit only bounded categorical log fields.

## Acceptance evidence

- A classified persistence rejection reaches `TargetCaptureStatus.failure` unchanged and keeps current-state browser control usable.
- The rejected frame declares one persistence gap; later gap or shutdown errors do not replace the first failure.
- Logs expose stage, code, operation, category, and recoverability without error debug text, paths, page data, or frame input.

## Ordering

Depends on the store classification checkpoint. Structured shutdown consumes the resulting capture failure.

## Implementation notes

- Execution capability: high; preserving first-cause semantics across concurrent capture, gap persistence, status, and logging is lifecycle-critical.
- Review weight: standard (caller/project default).
- Files changed: capture failure/status contracts in `krometrail-core`, capture pipeline/tests in `krometrail-cdp`, compile consumers, and this story.
- Tests added/removed: added a classified rejecting sink regression proving byte-for-byte first-cause retention, one persistence gap, later gap-failure non-overwrite, and privacy-safe status; replaced stage-only assertions with typed failure assertions.
- Simplification: deleted `failure_stage` state and `TargetCaptureStatus::new_with_failure_stage`; one `CaptureFailure` now owns stage and cause, and pending-gap persistence returns its error instead of a lossy boolean.
- Discrepancies from design: current-state control usability remains protected by the existing separation between capture coordinator and control operations; this checkpoint verifies capture failure does not escape its coordinator rather than duplicating a control test.
- Adjacent issues parked: none.
- Verification: focused core capture-status test; classified capture rejection test; `cargo check -p krometrail-core -p krometrail-cdp --all-targets`. Full CDP lib run reached 175 passing tests and four pre-existing local-socket tests blocked by sandbox `PermissionDenied`; no feature test failed.
