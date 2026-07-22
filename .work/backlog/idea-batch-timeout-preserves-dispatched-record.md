---
id: idea-batch-timeout-preserves-dispatched-record
created: 2026-07-22
updated: 2026-07-21
tags: [browser, bug]
---

A batch deadline that fires after a step's input dispatch drops the step's
entire future (`crates/krometrail-cdp/src/control/batch.rs` ~347), reporting a
timed-out step without the proven-dispatch interaction record — the record is
only constructed after observation. This contradicts the SPEC principle that
observation failure after a proven action must not erase the dispatch.

Surfaced by the cross-model review of
`epic-state-aware-interaction-results-postcondition-core` (finding 3): the
postcondition probes widened the post-dispatch window where the deadline can
bite; the review-fix made probing concurrent and budget-capped, which narrows
exposure back to roughly pre-epic levels, but the underlying gap is
pre-existing — any slow observation inside a batch deadline can still swallow
a dispatched step's record.

Fix direction: a dispatched step must always construct and persist its
interaction record (with degraded observation parts) even when the batch
deadline expires mid-observation — e.g. shield record construction/persistence
from the deadline, or split dispatch acknowledgement from observation so the
timeout path returns the pre-dispatch anchor with an explicit degraded
observation instead of dropping the future.
