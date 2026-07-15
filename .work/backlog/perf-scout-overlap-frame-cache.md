---
id: perf-scout-overlap-frame-cache
created: 2026-07-15
tags: [perf, idea, caching]
idea_origin: scout
idea_lens: caching
idea_borrowed_from: immutable image-operation caches
idea_priority: worth-a-look
idea_location: src/artifacts/decode.rs:36
idea_source: scout
---

# Cache decoded and normalized frames across overlapping queries

**Location**: `src/artifacts/decode.rs:36` · **Lens**: caching · **Borrowed from**: immutable image-operation caches · **Priority**: worth-a-look
**Leverage**: High · **Applicability**: Plausible · **Cost**: High

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Use a bounded byte-weighted cache keyed by source digest, decoder profile, geometry, normalization recipe, and algorithm/LUT version for sliding-window queries.

**Why it might help**: Nearby agent queries share many frames but current reuse ends with one flight; intermediate reuse might reduce first-hit latency for overlapping ranges.

**Validate by**: Measure overlap hit rate, decode/normalize calls, p95 latency, resident bytes, deletion/session lifecycle, and exact outputs.

**Risk**: Large buffers, stale session data, missing key fields, and capture starvation are significant risks.
