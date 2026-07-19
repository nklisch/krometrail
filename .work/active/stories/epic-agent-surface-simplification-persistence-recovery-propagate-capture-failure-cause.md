---
id: epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause
kind: story
stage: implementing
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
