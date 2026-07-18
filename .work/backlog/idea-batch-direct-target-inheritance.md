---
id: idea-batch-direct-target-inheritance
created: 2026-07-17
updated: 2026-07-17
tags: [bug, browser, agent-ux]
---

Krometrail v1.0.5 manual multi-tab testing shows that a batch with an explicit outer target rejects
targetless steps when the logical selected page is a different target. The Wikipedia batch admitted
the direct Wikipedia target, but its first targetless `fill` step failed with `target_failed` and
"batch step no longer resolves to the admitted target"; the remaining step was skipped. The same
targetless-step shape succeeded when run against the selected GitHub page. This contradicts the
shipped skill guidance that the outer page target applies by default.
