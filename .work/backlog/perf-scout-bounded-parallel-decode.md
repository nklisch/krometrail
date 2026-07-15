---
id: perf-scout-bounded-parallel-decode
created: 2026-07-15
tags: [perf, idea, parallelism, investigate-first]
idea_origin: scout
idea_lens: parallelism
idea_borrowed_from: bounded parallel ingestion pipelines
idea_priority: investigate-first
idea_location: src/artifacts/epoch.rs:191
idea_source: peer-glm5.2
---

# Decode retained frames concurrently under scheduler control

**Location**: `src/artifacts/epoch.rs:191` · **Lens**: parallelism · **Borrowed from**: bounded parallel ingestion pipelines · **Priority**: investigate-first
**Leverage**: High · **Applicability**: Likely · **Cost**: Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Replace serial frame decode with indexed bounded parallel decode using a scheduler-owned pool that leaves headroom for capture ingestion and collects results in source order.

**Why it might help**: This might reduce the serial decode component for 30–120-frame ranges before visual analysis begins.

**Validate by**: Benchmark 8/30/60/120-frame decode/end-to-end time, RSS, CPU, and capture queue latency at 1/2/4 workers.

**Risk**: Nested pools or oversubscription can starve capture; avoid unconstrained global Rayon.
