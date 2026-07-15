---
id: perf-scout-bounded-generator-fanout
created: 2026-07-15
tags: [perf, idea, parallelism]
idea_origin: scout
idea_lens: parallelism
idea_borrowed_from: pipeline fan-out/fan-in
idea_priority: worth-a-look
idea_location: src/artifacts/service.rs:392
idea_source: scout
---

# Fan out independent artifact generators within bounded budgets

**Location**: `src/artifacts/service.rs:392` · **Lens**: parallelism · **Borrowed from**: pipeline fan-out/fan-in · **Priority**: worth-a-look
**Leverage**: High · **Applicability**: Plausible · **Cost**: Medium-High

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: After shared decode and normalization, run independent generator groups concurrently under existing per-request limits and restore deterministic result/publication order.

**Why it might help**: This might overlap storyboard and difference-map CPU work instead of running outputs serially.

**Validate by**: Measure stage/end-to-end latency at one versus two generators plus RSS, CPU, capture queue depth, cancellation, and exact artifacts.

**Risk**: Oversubscription, memory spikes, inner parallelism, and ordering must remain controlled.
