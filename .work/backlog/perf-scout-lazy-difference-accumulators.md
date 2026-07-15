---
id: perf-scout-lazy-difference-accumulators
created: 2026-07-15
tags: [perf, idea, memory]
idea_origin: scout
idea_lens: memory-locality
idea_borrowed_from: sparse matrices and bitmap engines
idea_priority: worth-a-look
idea_location: crates/temporal-vision/src/difference_map.rs:125
idea_source: scout
---

# Lazily allocate sparse-change difference accumulators

**Location**: `crates/temporal-vision/src/difference_map.rs:125` · **Lens**: memory-locality · **Borrowed from**: sparse matrices and bitmap engines · **Priority**: worth-a-look
**Leverage**: High · **Applicability**: Plausible · **Cost**: Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Keep comparable counts dense but allocate change/timing arrays lazily or by active tile, with a measured density crossover to dense storage.

**Why it might help**: This might avoid allocating and zeroing roughly 100 MB of arrays for static or sparsely changing 1080p sequences.

**Validate by**: Measure 0/1/10/100% change density, allocations, RSS/cache misses, and exact output equivalence.

**Risk**: Representation branches, deterministic iteration, masks, and crossover overhead can erase the benefit.
