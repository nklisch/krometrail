---
id: perf-scout-share-pair-classification
created: 2026-07-15
tags: [perf, idea, algorithmic, investigate-first]
idea_origin: scout
idea_lens: algorithmic
idea_borrowed_from: HPC loop fusion and database materialized views
idea_priority: investigate-first
idea_location: crates/temporal-vision/src/measure.rs:242
idea_source: peer-glm5.2
---

# Reuse one pair-classification trace across generators

**Location**: `crates/temporal-vision/src/measure.rs:242` · **Lens**: algorithmic · **Borrowed from**: HPC loop fusion and database materialized views · **Priority**: investigate-first
**Leverage**: High · **Applicability**: Likely · **Cost**: Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Build a bounded request-scoped adjacent-pair classification trace once and feed storyboard measurement, difference accumulation, and motion history instead of rescanning the same normalized pair.

**Why it might help**: This might remove duplicate full-image passes whose cost scales with frame_count × pixels across multi-output bundles.

**Validate by**: Count pair scans and benchmark CPU/cache/RSS for 8/30/60/120 frames while requiring exact selections, accumulators, manifests, and PNG hashes.

**Risk**: Trace storage, gaps, masks, ties, and integer semantics must stay bounded and identical.
