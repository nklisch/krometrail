---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-retention-recovery-and-qualification
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest; this checkpoint qualified the integrated root service against the real schema-v4 store and closed errors discovered at the adapter/store boundary.
- Review weight: standard from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: `src/artifacts/{mod.rs,generators.rs,service_tests.rs,qualification_tests.rs}` and `crates/krometrail-store/tests/artifact_store.rs`.
- Integrated qualification: a real `RecordingStore` fixture persists mixed JPEG/PNG source frames with tied epoch-boundary time, two visual epochs, a declared gap, marker, and deterministic IDs. It proves exact typed-manifest JSON round-trip, deterministic PNG bytes and hashes, original-ID cache hits, corruption regeneration, and exact artifact usage.
- Corruption/recovery: runtime hit validation now has explicit cases for malformed manifest JSON, manifest hash, missing and mismatched ordered source links/hashes, output hash, and corrupt bytes. Existing file publication failpoints cover temp sync, rename, and directory sync; staging/ready recovery and a second reopen are idempotent.
- Retention/deletion: tiny-budget tests prove linked artifacts disappear before their source frames, pinned source segments survive independent artifact eviction/regeneration without pin-state mutation, and destructive session deletion removes source rows, links, artifact rows, files, and usage. A controlled loaded-frame barrier proves late CPU work cannot republish after deletion; pre-cancelled publication leaves no state.
- Scheduling/integration: controlled publication barriers prove active artifact work does not hold the recording mutation gate while a gap is persisted. Limit qualification covers exact and one-unit-over source count/bytes, pixels, decoded/normalized/combined memory, marker/output counts, per-output cap, deadline, cancellation, and permit independence. Temporal-vision resource-limit errors now preserve the public `ResourceLimitExceeded` classification instead of being flattened to generic generation failure.
- Manual ignored workload: the synthetic 24-frame 1920x1080 PNG case passed under Rust 1.85 and reported `encoded_source_bytes=1,026,744`, `decoded_rgba_bytes=199,065,600`, `artifact_bytes=33,303`, `uncached_ms=19,449`, `cached_ms=56`, `ingestion_us=160`, and one output on this host. These are workload-shape observations only, not live-Chrome evidence or CI speed thresholds.
- Verification: targeted rustfmt; Rust 1.85 locked workspace all-target format/check/test/Clippy `-D warnings` passed in an isolated tree that excluded concurrent uncommitted browser-event transport work. The final run covered 197 focused root/core/store tests plus the complete workspace suites; one manual workload remained ignored in normal CI and passed when invoked explicitly.
- Simplification: qualification reused the public service/store ports and authoritative temporal manifest; it added no second reader, cache, retention authority, SQL snapshot, or duplicate generator algorithm test.
- Discrepancies from design: no separate `artifact_recovery.rs` fixture was needed because publication file failpoints live beside the blocking worker and reopen/corruption behavior is covered through `artifact_store.rs`. The source-eviction pressure case validates linked removal and the production deletion batch query excludes unrelated artifacts by construction; it does not add a test-only store eviction API solely to target a particular segment. Total-output enforcement is covered in production aggregation and count/per-output boundary tests; tiny deterministic outputs cannot naturally approach the 256 MiB default total without turning qualification into a resource-stress test.
- Adjacent issues fixed: temporal-vision `ResourceLimitExceeded` was mapped to the matching core code so callers can distinguish configured ceilings from malformed generator requests.
- Adjacent issues parked: none.
