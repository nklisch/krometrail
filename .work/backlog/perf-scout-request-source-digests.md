---
id: perf-scout-request-source-digests
created: 2026-07-15
tags: [perf, idea, caching, investigate-first]
idea_origin: scout
idea_lens: caching
idea_borrowed_from: database dataloaders and request-scoped caches
idea_priority: investigate-first
idea_location: crates/krometrail-store/src/recording.rs:331
idea_source: peer-glm5.2
---

# Memoize source digests and batch validation within one request

**Location**: `crates/krometrail-store/src/recording.rs:331` · **Lens**: caching · **Borrowed from**: database dataloaders and request-scoped caches · **Priority**: investigate-first
**Leverage**: High · **Applicability**: Likely · **Cost**: Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Reuse validated frame payload digests and bulk artifact lookups within one multi-output request while retaining deletion fences and final publication revalidation.

**Why it might help**: This might collapse repeated per-output hashing and store round-trips without trusting persisted checksums across requests.

**Validate by**: Instrument hash calls/bytes, SQL statements, reads, p95 latency, corruption injection, deletion races, and exact output identities.

**Risk**: Request-local proofs must not outlive the request or weaken integrity and deletion checks.
