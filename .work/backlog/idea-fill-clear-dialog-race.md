---
id: idea-fill-clear-dialog-race
created: 2026-07-14
updated: 2026-07-14
tags: [browser, testing]
---

Evaluate whether Fill's `Ctrl+A` plus `Delete` clear sequence should use the same eager-poll dispatch posture as click and drag gestures. Today the two key commands are awaited sequentially, so a dialog opening between them can leave only select-all dispatched; the operation reports the resulting observation failure honestly, but the gesture is not atomic in the same sense as pointer dispatch. Reproduce the dialog race before changing behavior, then either preserve and document the asymmetry or add a focused regression and bounded fix.
