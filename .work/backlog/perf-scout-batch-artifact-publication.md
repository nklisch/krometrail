---
id: perf-scout-batch-artifact-publication
created: 2026-07-15
tags: [perf, idea, io]
idea_origin: scout
idea_lens: io-batching
idea_borrowed_from: storage-engine group commit
idea_priority: worth-a-look
idea_location: crates/krometrail-store/src/artifacts/files.rs:135
idea_source: peer-glm5.2
---

# Batch durable publication for multi-output requests

**Location**: `crates/krometrail-store/src/artifacts/files.rs:135` · **Lens**: io-batching · **Borrowed from**: storage-engine group commit · **Priority**: worth-a-look
**Leverage**: High · **Applicability**: Plausible · **Cost**: Low-Medium

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Stage and sync artifact temp files, rename as a batch, sync the directory once, then finalize exact metadata with per-artifact receipts.

**Why it might help**: This might amortize repeated fsync/rename/directory-sync work on multi-output requests.

**Validate by**: Count syscalls/fsyncs and benchmark publication with failpoints, restart recovery, deletion races, and identity checks.

**Risk**: Partial-batch recovery and durability ordering must remain exact.
