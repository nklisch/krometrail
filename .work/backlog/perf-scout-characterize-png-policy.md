---
id: perf-scout-characterize-png-policy
created: 2026-07-15
tags: [perf, idea, runtime]
idea_origin: scout
idea_lens: compiler-runtime
idea_borrowed_from: codec engineering
idea_priority: worth-a-look
idea_location: crates/temporal-vision/src/encode.rs:26
idea_source: scout
---

# Characterize deterministic PNG compression and filter policy

**Location**: `crates/temporal-vision/src/encode.rs:26` · **Lens**: compiler-runtime · **Borrowed from**: codec engineering · **Priority**: worth-a-look
**Leverage**: High · **Applicability**: Plausible · **Cost**: Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Benchmark Best/Default/Fast compression and deterministic filters on flat, text-heavy, sparse-change, and noisy outputs; adopt only a measured versioned policy.

**Why it might help**: A different exact encoding policy might reduce CPU and possibly durable bytes for generated canvases.

**Validate by**: Measure encode-only and end-to-end time, bytes, disk budget, decoded-pixel equality, and required hash/version changes.

**Risk**: Encoded bytes, hashes, descriptors, and cache identity change.
