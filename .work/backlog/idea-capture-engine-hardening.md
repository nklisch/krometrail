---
id: idea-capture-engine-hardening
created: 2026-07-13
updated: 2026-07-13
tags: [browser, testing]
---

Lower-risk follow-ups from the bounded capture-engine review:

- Preserve or explicitly aggregate exact estimated counts when a full gap ledger coalesces mixed count-bearing and non-count gap reasons; current behavior honestly degrades to unknown rather than fabricating continuity.
- Normalize `FrameRejected` estimated-count behavior between reader-side and worker-side rejection.
- Make the private coordinator's active-stream cap robust to concurrent `start_target` calls even though production wiring is a single lifecycle owner.
- Replace the saturation test's current-thread scheduling assumption with an explicit barrier if the test runtime becomes multi-threaded.
- Remove or wire the currently unused `StartedHidden` transition after supervised visibility wiring settles.

These do not compromise current boundedness, acknowledgement ordering, privacy, or loss honesty and are deferred outside active feature scope.
