---
id: perf-scout-opaque-row-normalization
created: 2026-07-15
tags: [perf, idea, memory, investigate-first]
idea_origin: scout
idea_lens: memory-locality
idea_borrowed_from: data-oriented image kernels
idea_priority: investigate-first
idea_location: crates/temporal-vision/src/normalize.rs:484
idea_source: peer-glm5.2
---

# Specialize normalization for opaque row-major input

**Location**: `crates/temporal-vision/src/normalize.rs:484` · **Lens**: memory-locality · **Borrowed from**: data-oriented image kernels · **Priority**: investigate-first
**Leverage**: High · **Applicability**: Likely · **Cost**: Low-Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Hoist scale mode out of the pixel loop, write pre-sized row slices, and use a predictable in-loop opaque-alpha path while retaining exact general-alpha handling.

**Why it might help**: This might remove repeated dispatch, blend arithmetic, coordinate work, and per-pixel vector bookkeeping on common opaque screenshots.

**Validate by**: Profile normalization and require byte-identical buffers/artifacts across alpha, crop, mask, identity, and downscale cases.

**Risk**: Preserve linear-light compositing, checked bounds, rounding, and provenance.
