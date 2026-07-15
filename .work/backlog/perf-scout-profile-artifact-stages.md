---
id: perf-scout-profile-artifact-stages
created: 2026-07-15
tags: [perf, idea, runtime, investigate-first]
idea_origin: scout
idea_lens: compiler-runtime
idea_borrowed_from: benchmark engineering / roofline analysis
idea_priority: investigate-first
idea_location: src/app/live_evaluation/latency.rs:99
idea_source: peer-glm5.2
---

# Optimized multi-frame stage profiling and cache isolation

**Location**: `src/app/live_evaluation/latency.rs:99` · **Lens**: compiler-runtime · **Borrowed from**: benchmark engineering / roofline analysis · **Priority**: investigate-first
**Leverage**: High · **Applicability**: Likely · **Cost**: Low

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Add optimized benchmark/release stage timers for store read, decode, normalize, pair analysis, selection, render, encode/hash, and publish across isolated all-cold, mixed, and all-warm cache namespaces. Exercise 2/8/30/60/120 1080p frames.

**Why it might help**: This might prevent optimizing an opt-level=0 artifact or the wrong stage and supplies the baseline required to judge every later candidate.

**Validate by**: Measure p50/p95 wall time, CPU, RSS/allocations, bytes hashed/read/written, stage counts, and capture-ingestion latency.

**Risk**: Instrumentation can perturb timings; keep it benchmark-only and bounded.
