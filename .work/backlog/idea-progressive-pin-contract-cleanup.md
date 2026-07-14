---
id: idea-progressive-pin-contract-cleanup
created: 2026-07-14
updated: 2026-07-14
tags: [refactor, storage]
---

Consolidate pin-contract cleanup after progressive evidence has real consumers. The store producer and `PinState` validator currently duplicate the same range-coalescing algorithm; make one implementation authoritative without weakening independent invariant checks. Also decide whether the legacy `RetentionStore::pin_range` / `unpin_range` and simpler recording `PinChange` can be removed now that production uses resolved-range pin operations, or explicitly document the legacy surface if another caller still earns it. Keep behavior, pin rows, overlap/idempotence, budget enforcement, and public evidence semantics unchanged.
