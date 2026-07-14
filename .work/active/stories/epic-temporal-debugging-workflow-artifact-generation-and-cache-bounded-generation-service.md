---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-bounded-generation-service
kind: story
stage: implementing
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