---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-bounded-generation-service
kind: story
stage: done
tags: [visual, storage]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache-frame-adaptation-and-decoding]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Compose the Bounded Generation Service

## Checkpoint

Implement the root `ArtifactGeneration` adapter over the existing frame source, temporal-vision generators, and `RecordingStore` artifact port. Enforce independent global request/CPU/memory permits, per-request generator concurrency, source/dimension/pixel/output counts and bytes, wall deadline, and cooperative cancellation. Generate deterministic epoch/request/kind-ordered outputs and use cache lookup plus process-wide single flight before publishing exact artifacts.

Support storyboard with optional existing orientation output, difference map, fixed-region filmstrip, and opt-in motion history. Storyboard/difference/motion share normalized epoch work; region filmstrip retains its existing typed fixed-region renderer. The root composition exposes the service for downstream bundle/MCP features without adding those features here.

## Files

- `src/artifacts/{service.rs,scheduler.rs,generators.rs,single_flight.rs,tests.rs}` (new)
- `src/{main.rs,app.rs}`
- root `Cargo.toml`

## Acceptance evidence

- Global active-request, blocking-CPU, weighted-memory, and per-request permits hit exact boundaries independently.
- `FitLimits` chooses the smallest exact divisor in `1,2,4,8`; explicit scale is never changed silently.
- Each supported request maps to the existing temporal-vision generator with exact parameters and manifest kind.
- Identical concurrent misses decode/generate once; cache is rechecked by the leader; publication uniqueness returns one winner.
- Waiter cancellation/deadline semantics are deterministic, last-waiter cancellation suppresses publication, and a running bounded blocking unit cannot publish after cancellation without another waiter.
- Parallel permit counts do not change result/manifest/output order or hashes.
- Saturated generation does not hold the recording mutation gate and a controlled concurrent frame/gap append completes.
- Root removes the no-op temporal-vision import and wires one shared frame/store/ID/service authority; MCP remains unchanged.

## Ordering

Depends on decoded epoch adaptation and the already-complete artifact store publication boundary. Integrated retention/recovery qualification follows.

## Implementation notes

- Execution capability: highest; this checkpoint combines bounded async/blocking scheduling, exact generator adaptation, cache lookup, single flight, cancellation, and root composition.
- Review weight: standard from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: root `Cargo.toml`, `src/app.rs`, `src/artifacts/{mod.rs,epoch.rs,generators.rs,scheduler.rs,service.rs,single_flight.rs,service_tests.rs,tests.rs}`, `crates/krometrail-core/src/ports/artifacts.rs`, `crates/krometrail-store/src/{artifacts/files.rs,recording.rs}`, and the adjacent exhaustiveness repair in `crates/krometrail-store/src/index/timeline.rs`.
- Tests added: all four generator families plus optional storyboard orientation in deterministic order; repeat cache identity/IDs/bytes; concurrent equal misses with one ID/publication set; past deadline and caller cancellation; ordered `AllowPartial` across epoch-local reference failure; exact `FitLimits` factor materialization in provenance; independent request/CPU/memory/generator permit saturation; last-waiter-only shared cancellation. Existing store tests cover externally cancelled publication suppression and file/store isolation.
- Verification: targeted rustfmt; all-target check for root/core/store; all root/core/store tests (190 passed); all-target Clippy with `-D warnings` for root/core/store (green).
- Scheduling semantics: validated independent global request, blocking-CPU, weighted-memory, output/count, and per-request generator ceilings; all decode/normalize/render runs in `spawn_blocking` behind the CPU semaphore, with one logical processor left by default where possible. Memory reservation spans retained decoded/normalized/output work. Deterministic preassigned slots preserve epoch/request/kind order regardless of permit timing.
- Cache/single-flight semantics: metadata planning and exact encoded hashing occur before cache lookup without decoding; ready hits avoid image work. Missing ordered cache keys join one process-wide flight, the leader rechecks cache, decodes each required epoch once, shares normalized sequences by canonical effective normalization, and publishes exact manifests/bytes. Waiters have independent deadlines/cancellation; only the last departing waiter cancels shared work, and the store receives that cancellation so late file work cannot become ready.
- Generator semantics: `FitLimits` materializes the smallest exact divisor in `1,2,4,8`; explicit scales never change. Storyboard/orientation, difference map, fixed-region filmstrip, and motion history call only existing temporal-vision APIs with typed parameters and effective runtime limits.
- Root composition: removed the no-op temporal-vision import and injects one shared `FrameSource`, `RecordingStore` artifact port, process ID source, scheduler, and `ArtifactGeneration` service. MCP and capture remain unaware of artifact computation.
- Simplification: metadata planning was split from decode so cache hits and single-flight followers do no image work; no decoded cache, second reader, second retention policy, or second manifest was introduced.
- Discrepancies from design: generator jobs currently execute deterministically in sequence while still acquiring the independent per-request and global CPU permits; this uses the same bounded contract without parallel scheduling complexity. `ArtifactPublication` gained an optional runtime-neutral cancellation signal so last-waiter suppression is enforceable through the atomic store publication boundary.
- Adjacent issues fixed: the concurrently landed browser-event observation variant left the store timeline sort-key match non-exhaustive; one typed ID arm was added so the workspace compiled. No browser-event route, persistence, or presentation behavior was added here.
- Adjacent issues parked: none.
