---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-retention-recovery-and-qualification
kind: story
stage: implementing
tags: [visual, storage, testing]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache-bounded-generation-service]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify Artifact Retention, Recovery, and Cache Semantics

## Checkpoint

Qualify the complete resolved-range → encoded-frame → temporal-vision → durable-cache path against real v4 storage. Cover deterministic cache/output/manifests, malformed and mixed-epoch inputs, all scheduler limits, cancellation/single-flight, crash recovery, corruption regeneration, usage, source eviction, pin behavior, and session-deletion races. This checkpoint closes integration defects but does not add bundle composition, MCP surfaces, browser events, diagnosis, replay, or comparison.

## Files

- `crates/krometrail-store/src/{recording.rs,recovery.rs}` and artifact/index modules only for discovered qualification fixes
- `crates/krometrail-store/tests/{artifact_store.rs,artifact_recovery.rs,retention_small_budget.rs}`
- `src/artifacts/tests.rs`
- `tests/fixtures/artifacts/` golden inputs

## Acceptance evidence

- Fixed inputs/IDs/parameters produce stable cache key, exact typed-manifest round-trip, exact output SHA-256, and deterministic PNG bytes.
- Every required cache-key input causes a miss when changed; exact repeats return original ID/bytes/manifest.
- Missing/corrupt bytes, manifest, source links, and hashes invalidate and regenerate rather than returning a false hit.
- Evicting any source removes every mixed-source staging/ready artifact before frame rows; unrelated complete artifacts survive.
- Pins protect source segments but not regenerable artifacts; independent artifact eviction/regeneration does not alter pin state.
- Session deletion cancels/drains active publication and leaves no source, artifact, staging/temp/final file, cache/source-link, or usage state; late CPU work cannot recreate it.
- Publication/deletion crash points converge after reopen, usage accounts exact artifact bytes once, and recovery's second pass is a no-op.
- Mixed epochs/gaps/markers, `RequireAll`/`AllowPartial`, all count/byte/pixel/memory/output limits, cancellation/deadline, single-flight, and no-ingestion-starvation scenarios pass deterministic tests.
- A manual ignored 1080p workload reports EVALUATION latency/memory/CPU/ingestion metrics without host-speed CI assertions.
- Rust 1.85 locked format/check/test/Clippy gates pass; tests avoid wrapper/SQL snapshots and duplicate temporal-vision algorithm coverage.

## Ordering

Depends on the fully composed service. Green verification advances this child directly to done and makes the parent feature eligible for standard feature-level review.